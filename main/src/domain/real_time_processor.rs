use crate::domain::{
    CompoundMappingSource, CompoundMappingSourceValue, ControlMainTask, ControlOptions,
    MappingCompartment, MappingId, MidiClockCalculator, NormalMainTask, PartialControlMatch,
    RealTimeMapping, SourceScanner, UnresolvedCompoundMappingTarget, VirtualSourceValue,
};
use helgoboss_learn::{ControlValue, MidiSourceValue};
use helgoboss_midi::{
    Channel, ControlChange14BitMessage, ControlChange14BitMessageScanner, DataEntryByteOrder,
    ParameterNumberMessage, PollingParameterNumberMessageScanner, RawShortMessage, ShortMessage,
    ShortMessageType,
};
use reaper_high::{MidiInputDevice, MidiOutputDevice, Reaper};
use reaper_medium::{Hz, MidiFrameOffset, SendMidiTime};
use slog::debug;
use std::collections::{HashMap, HashSet};

use crate::core::Global;
use enum_iterator::IntoEnumIterator;
use enum_map::{enum_map, EnumMap};
use std::ptr::null_mut;
use std::time::Duration;
use vst::api::{EventType, Events, MidiEvent};
use vst::host::Host;
use vst::plugin::HostCallback;

const NORMAL_BULK_SIZE: usize = 100;
const FEEDBACK_BULK_SIZE: usize = 100;

#[derive(PartialEq, Debug)]
pub(crate) enum ControlMode {
    Disabled,
    Controlling,
    LearningSource(MappingCompartment),
}

impl ControlMode {
    fn is_learning(&self) -> bool {
        matches!(self, ControlMode::LearningSource(_))
    }
}

#[derive(Debug)]
pub struct RealTimeProcessor {
    instance_id: String,
    logger: slog::Logger,
    // Synced processing settings
    control_mode: ControlMode,
    midi_control_input: MidiControlInput,
    midi_feedback_output: Option<MidiFeedbackOutput>,
    mappings: EnumMap<MappingCompartment, HashMap<MappingId, RealTimeMapping>>,
    let_matched_events_through: bool,
    let_unmatched_events_through: bool,
    // Inter-thread communication
    normal_task_receiver: crossbeam_channel::Receiver<NormalRealTimeTask>,
    feedback_task_receiver: crossbeam_channel::Receiver<FeedbackRealTimeTask>,
    normal_main_task_sender: crossbeam_channel::Sender<NormalMainTask>,
    control_main_task_sender: crossbeam_channel::Sender<ControlMainTask>,
    // Scanners for more complex MIDI message types
    nrpn_scanner: PollingParameterNumberMessageScanner,
    cc_14_bit_scanner: ControlChange14BitMessageScanner,
    // For source learning
    source_scanner: SourceScanner,
    // For MIDI timing clock calculations
    midi_clock_calculator: MidiClockCalculator,
}

impl RealTimeProcessor {
    pub fn new(
        instance_id: String,
        parent_logger: &slog::Logger,
        normal_task_receiver: crossbeam_channel::Receiver<NormalRealTimeTask>,
        feedback_task_receiver: crossbeam_channel::Receiver<FeedbackRealTimeTask>,
        normal_main_task_sender: crossbeam_channel::Sender<NormalMainTask>,
        control_main_task_sender: crossbeam_channel::Sender<ControlMainTask>,
    ) -> RealTimeProcessor {
        use MappingCompartment::*;
        RealTimeProcessor {
            instance_id,
            logger: parent_logger.new(slog::o!("struct" => "RealTimeProcessor")),
            control_mode: ControlMode::Controlling,
            normal_task_receiver,
            feedback_task_receiver,
            normal_main_task_sender,
            control_main_task_sender,
            mappings: enum_map! {
                ControllerMappings => HashMap::with_capacity(100),
                MainMappings => HashMap::with_capacity(500),
            },
            let_matched_events_through: false,
            let_unmatched_events_through: false,
            nrpn_scanner: PollingParameterNumberMessageScanner::new(Duration::from_millis(1)),
            cc_14_bit_scanner: Default::default(),
            midi_control_input: MidiControlInput::FxInput,
            midi_feedback_output: None,
            source_scanner: Default::default(),
            midi_clock_calculator: Default::default(),
        }
    }

    pub fn process_incoming_midi_from_vst(
        &mut self,
        frame_offset: MidiFrameOffset,
        msg: RawShortMessage,
        is_reaper_generated: bool,
        host: &HostCallback,
    ) {
        if self.midi_control_input == MidiControlInput::FxInput {
            if is_reaper_generated {
                // Ignore note off messages which are a result of starting the transport. They
                // are generated by REAPER in order to stop instruments from sounding. But ReaLearn
                // is not an instrument in the classical sense. We don't want to reset target values
                // just because play has been pressed!
                self.process_unmatched_short(msg, Caller::Vst(host));
                return;
            }
            self.process_incoming_midi(frame_offset, msg, Caller::Vst(host));
        } else {
            // #33 Even though MIDI input device is not set to <FX input>, we want to be able to
            // influence whether messages are let through or not. In this case, FX input events
            // are always unmatched.
            if self.let_unmatched_events_through {
                self.send_midi_to_fx_output(msg, Caller::Vst(host))
            }
        }
    }

    pub fn run_from_vst(&mut self, _sample_count: usize, host: &HostCallback) {
        if self.get_feedback_driver() == Driver::Vst {
            self.process_feedback_tasks(Caller::Vst(host));
        }
    }

    pub fn run_from_audio_hook(&mut self, sample_count: usize) {
        if self.get_feedback_driver() == Driver::AudioHook {
            self.process_feedback_tasks(Caller::AudioHook);
        }
        self.run(sample_count, Caller::AudioHook);
    }

    /// There's an important difference between using audio hook or VST plug-in as driver:
    /// VST processing stops e.g. when project paused and track not armed or on input FX chain and
    /// track not armed. The result is that control, feedback, mapping updates and many other things
    /// wouldn't work anymore. That's why we prefer audio hook whenever possible. However, we can't
    /// use the audio hook if we need access to the VST plug-in host callback because it's dangerous
    /// (would crash when plug-in gone) and somehow strange (although it seems to work).
    ///
    /// **IMPORTANT**: If "MIDI control input" is set to a MIDI device, it's very important that
    /// `run()` is called either just from the VST or just from the audio hook. If both do it,
    /// the MIDI messages are processed **twice**!!! Easy solution: Never have two drivers.
    fn get_feedback_driver(&self) -> Driver {
        use Driver::*;
        match self.midi_feedback_output {
            // Feedback not sent at all. We still want to "eat" any remaining feedback messages.
            // We do everything in the audio hook because it's more reliable.
            None => AudioHook,
            // Feedback sent directly to device. Same here: We let the audio hook do everything in
            // order to not run into surprising situations where control or feedback don't work.
            Some(MidiFeedbackOutput::Device(_)) => AudioHook,
            // Feedback sent to FX output. Here we have to be more careful because sending feedback
            // to FX output involves host callback invocation. This can only be done from the VST
            // plug-in.
            // TODO-medium Feedback tasks can queue up if VST processing stopped! Maybe we should
            //  detect somehow if stopped and switch to audio hook in that case or stop sending?
            Some(MidiFeedbackOutput::FxOutput) => Vst,
        }
    }

    /// Should be called regularly in real-time audio thread.
    fn run(&mut self, sample_count: usize, caller: Caller) {
        // Increase MIDI clock calculator's sample counter
        self.midi_clock_calculator
            .increase_sample_counter_by(sample_count as u64);
        // Process occasional tasks sent from other thread (probably main thread)
        let normal_task_count = self.normal_task_receiver.len();
        for task in self.normal_task_receiver.try_iter().take(NORMAL_BULK_SIZE) {
            use NormalRealTimeTask::*;
            match task {
                UpdateAllMappings(compartment, mappings) => {
                    debug!(
                        self.logger,
                        "Updating {} {}...",
                        mappings.len(),
                        compartment
                    );
                    self.mappings[compartment].clear();
                    for m in mappings.into_iter() {
                        self.mappings[compartment].insert(m.id(), m);
                    }
                }
                UpdateSingleMapping(compartment, mapping) => {
                    debug!(
                        self.logger,
                        "Updating single {} {:?}...",
                        compartment,
                        mapping.id()
                    );
                    self.mappings[compartment].insert(mapping.id(), *mapping);
                }
                UpdateTargetActivations(compartment, mappings_with_active_target) => {
                    // TODO-low We should use an own logger and always log the sample count
                    //  automatically.
                    // Also log sample count in order to be sure about invocation order
                    // (timestamp is not accurate enough on e.g. selection changes).
                    debug!(
                        self.logger,
                        "Update target activations for {} {} at {} samples...",
                        mappings_with_active_target.len(),
                        compartment,
                        self.midi_clock_calculator.current_sample_count()
                    );
                    for m in self.mappings[compartment].values_mut() {
                        m.update_target_activation(mappings_with_active_target.contains(&m.id()));
                    }
                }
                UpdateSettings {
                    let_matched_events_through,
                    let_unmatched_events_through,
                    midi_control_input,
                    midi_feedback_output,
                } => {
                    debug!(self.logger, "Updating settings");
                    self.let_matched_events_through = let_matched_events_through;
                    self.let_unmatched_events_through = let_unmatched_events_through;
                    self.midi_control_input = midi_control_input;
                    self.midi_feedback_output = midi_feedback_output;
                }
                UpdateSampleRate(sample_rate) => {
                    debug!(self.logger, "Updating sample rate");
                    self.midi_clock_calculator.update_sample_rate(sample_rate);
                }
                StartLearnSource(compartment) => {
                    debug!(self.logger, "Start learning source");
                    self.control_mode = ControlMode::LearningSource(compartment);
                    self.nrpn_scanner.reset();
                    self.cc_14_bit_scanner.reset();
                    self.source_scanner.reset();
                }
                DisableControl => {
                    debug!(self.logger, "Disable control");
                    self.control_mode = ControlMode::Disabled;
                }
                ReturnToControlMode => {
                    debug!(self.logger, "Return to control mode");
                    self.control_mode = ControlMode::Controlling;
                    self.nrpn_scanner.reset();
                    self.cc_14_bit_scanner.reset();
                }
                LogDebugInfo => {
                    self.log_debug_info(normal_task_count);
                }
                UpdateMappingActivations(compartment, activation_updates) => {
                    debug!(self.logger, "Update mapping activations...");
                    for update in activation_updates.into_iter() {
                        if let Some(m) = self.mappings[compartment].get_mut(&update.id) {
                            m.update_activation(update.is_active);
                        } else {
                            panic!(
                                "Couldn't find real-time mapping while updating mapping activations"
                            );
                        }
                    }
                }
            }
        }
        // Read MIDI events from devices
        if let MidiControlInput::Device(dev) = self.midi_control_input {
            dev.with_midi_input(|mi| {
                for evt in mi.get_read_buf().enum_items(0) {
                    self.process_incoming_midi(
                        evt.frame_offset(),
                        evt.message().to_other(),
                        caller,
                    );
                }
            });
        }
        // Poll (N)RPN scanner
        for ch in 0..16 {
            if let Some(nrpn_msg) = self.nrpn_scanner.poll(Channel::new(ch)) {
                self.process_incoming_midi_normal_nrpn(nrpn_msg, caller);
            }
        }
        // Poll source scanner if we are learning a source currently
        if self.control_mode.is_learning() {
            self.poll_source_scanner()
        }
    }

    fn process_feedback_tasks(&self, caller: Caller) {
        // Process (frequent) feedback tasks sent from other thread (probably main thread)
        for task in self
            .feedback_task_receiver
            .try_iter()
            .take(FEEDBACK_BULK_SIZE)
        {
            use FeedbackRealTimeTask::*;
            match task {
                Feedback(source_value) => {
                    use CompoundMappingSourceValue::*;
                    match source_value {
                        Midi(v) => self.feedback_midi(v, caller),
                        Virtual(v) => self.feedback_virtual(v, caller),
                    };
                }
            }
        }
    }

    fn log_debug_info(&self, task_count: usize) {
        // Summary
        let msg = format!(
            "\n\
            # Real-time processor\n\
            \n\
            - State: {:?} \n\
            - Total main mapping count: {} \n\
            - Enabled main mapping count: {} \n\
            - Total controller mapping count: {} \n\
            - Enabled controller mapping count: {} \n\
            - Normal task count: {} \n\
            - Feedback task count: {} \n\
            ",
            self.control_mode,
            self.mappings[MappingCompartment::MainMappings].len(),
            self.mappings[MappingCompartment::MainMappings]
                .values()
                .filter(|m| m.control_is_effectively_on())
                .count(),
            self.mappings[MappingCompartment::ControllerMappings].len(),
            self.mappings[MappingCompartment::ControllerMappings]
                .values()
                .filter(|m| m.control_is_effectively_on())
                .count(),
            task_count,
            self.feedback_task_receiver.len(),
        );
        Global::task_support()
            .do_in_main_thread_asap(move || {
                Reaper::get().show_console_msg(msg);
            })
            .unwrap();
        // Detailled
        println!(
            "\n\
            # Real-time processor\n\
            \n\
            {:#?}
            ",
            self
        );
    }

    fn process_incoming_midi(
        &mut self,
        frame_offset: MidiFrameOffset,
        msg: RawShortMessage,
        caller: Caller,
    ) {
        use ShortMessageType::*;
        match msg.r#type() {
            NoteOff
            | NoteOn
            | PolyphonicKeyPressure
            | ControlChange
            | ProgramChange
            | ChannelPressure
            | PitchBendChange
            | Start
            | Continue
            | Stop => {
                self.process_incoming_midi_normal(msg, caller);
            }
            SystemExclusiveStart
            | TimeCodeQuarterFrame
            | SongPositionPointer
            | SongSelect
            | SystemCommonUndefined1
            | SystemCommonUndefined2
            | TuneRequest
            | SystemExclusiveEnd
            | SystemRealTimeUndefined1
            | SystemRealTimeUndefined2
            | ActiveSensing
            | SystemReset => {
                // ReaLearn doesn't process those. Forward them if user wants it.
                self.process_unmatched_short(msg, caller);
            }
            TimingClock => {
                // Timing clock messages are treated special (calculates BPM).
                if let Some(bpm) = self.midi_clock_calculator.feed(frame_offset) {
                    let source_value = MidiSourceValue::<RawShortMessage>::Tempo(bpm);
                    self.control_midi(source_value);
                }
            }
        };
    }

    /// This basically splits the stream of short MIDI messages into 3 streams:
    ///
    /// - (N)RPN messages
    /// - 14-bit CC messages
    /// - Short MIDI messaages
    fn process_incoming_midi_normal(&mut self, msg: RawShortMessage, caller: Caller) {
        // TODO-low This is probably unnecessary optimization, but we could switch off
        //  NRPN/CC14 scanning if there's no such source.
        if let Some(nrpn_msg) = self.nrpn_scanner.feed(&msg) {
            self.process_incoming_midi_normal_nrpn(nrpn_msg, caller);
        }
        if let Some(cc14_msg) = self.cc_14_bit_scanner.feed(&msg) {
            self.process_incoming_midi_normal_cc14(cc14_msg, caller);
        }
        // Even if an composite message ((N)RPN or CC 14-bit) was scanned, we still process the
        // plain short MIDI message. This is desired. Rationale: If there's no mapping with a
        // composite source of this kind, then all the CCs potentially involved in composite
        // messages can still be used separately (e.g. CC 6, 38, 98, etc.). That's important!
        // However, if there's at least one mapping source that listens to composite messages
        // of the incoming kind, we need to make sure that the single messages can't be used
        // anymore! Otherwise it would be confusing. They are consumed. That's the reason why
        // we do the consumption check at a later state.
        self.process_incoming_midi_normal_plain(msg, caller);
    }

    fn process_incoming_midi_normal_nrpn(&mut self, msg: ParameterNumberMessage, caller: Caller) {
        let source_value = MidiSourceValue::<RawShortMessage>::ParameterNumber(msg);
        match self.control_mode {
            ControlMode::Controlling => {
                let matched = self.control_midi(source_value);
                if self.midi_control_input != MidiControlInput::FxInput {
                    return;
                }
                if (matched && self.let_matched_events_through)
                    || (!matched && self.let_unmatched_events_through)
                {
                    for m in msg
                        .to_short_messages::<RawShortMessage>(DataEntryByteOrder::MsbFirst)
                        .iter()
                        .flatten()
                    {
                        self.send_midi_to_fx_output(*m, caller);
                    }
                }
            }
            ControlMode::LearningSource(compartment) => {
                self.feed_source_scanner(source_value, compartment);
            }
            ControlMode::Disabled => {}
        }
    }

    fn poll_source_scanner(&mut self) {
        if let Some(source) = self.source_scanner.poll() {
            self.learn_source(source);
        }
    }

    fn feed_source_scanner(
        &mut self,
        value: MidiSourceValue<RawShortMessage>,
        compartment: MappingCompartment,
    ) {
        let compound_value = if compartment == MappingCompartment::ControllerMappings {
            // Controller mappings can't have virtual sources, so we also don't want to learn them.
            CompoundMappingSourceValue::Midi(value)
        } else {
            // All other mappings can have virtual sources and they should be preferred over direct
            // ones.
            self.virtualize_if_possible(value)
        };
        if let Some(source) = self.source_scanner.feed(compound_value) {
            self.learn_source(source);
        }
    }

    fn virtualize_if_possible(
        &mut self,
        value: MidiSourceValue<RawShortMessage>,
    ) -> CompoundMappingSourceValue {
        // If this MIDI source value translates to a virtual source value, return the first match.
        for m in self.mappings[MappingCompartment::ControllerMappings]
            .values_mut()
            .filter(|m| m.control_is_effectively_on())
        {
            if let Some(control_match) = m.control(value) {
                use PartialControlMatch::*;
                if let ProcessVirtual(virtual_source_value) = control_match {
                    return CompoundMappingSourceValue::Virtual(virtual_source_value);
                };
            }
        }
        // Otherwise just return the MIDI source value as is.
        CompoundMappingSourceValue::Midi(value)
    }

    fn learn_source(&mut self, source: CompoundMappingSource) {
        // If plug-in dropped, the receiver might be gone already because main processor is
        // unregistered synchronously.
        let _ = self
            .normal_main_task_sender
            .send(NormalMainTask::LearnSource(source));
    }

    fn process_incoming_midi_normal_cc14(
        &mut self,
        msg: ControlChange14BitMessage,
        caller: Caller,
    ) {
        let source_value = MidiSourceValue::<RawShortMessage>::ControlChange14Bit(msg);
        match self.control_mode {
            ControlMode::Controlling => {
                let matched = self.control_midi(source_value);
                if self.midi_control_input != MidiControlInput::FxInput {
                    return;
                }
                if (matched && self.let_matched_events_through)
                    || (!matched && self.let_unmatched_events_through)
                {
                    for m in msg.to_short_messages::<RawShortMessage>().iter() {
                        self.send_midi_to_fx_output(*m, caller);
                    }
                }
            }
            ControlMode::LearningSource(compartment) => {
                self.feed_source_scanner(source_value, compartment);
            }
            ControlMode::Disabled => {}
        }
    }

    fn process_incoming_midi_normal_plain(&mut self, msg: RawShortMessage, caller: Caller) {
        let source_value = MidiSourceValue::Plain(msg);
        match self.control_mode {
            ControlMode::Controlling => {
                if self.is_consumed_by_at_least_one_source(msg) {
                    // Some short MIDI messages are just parts of bigger composite MIDI messages,
                    // e.g. (N)RPN or 14-bit CCs. If we reach this point, the incoming message
                    // could potentially match one of the (N)RPN or 14-bit CC mappings in the list
                    // and therefore doesn't qualify anymore as a candidate for normal CC sources.
                    return;
                }
                let matched = self.control_midi(source_value);
                if matched {
                    self.process_matched_short(msg, caller);
                } else {
                    self.process_unmatched_short(msg, caller);
                }
            }
            ControlMode::LearningSource(compartment) => {
                self.feed_source_scanner(source_value, compartment);
            }
            ControlMode::Disabled => {}
        }
    }

    fn all_mappings(&self) -> impl Iterator<Item = &RealTimeMapping> {
        MappingCompartment::into_enum_iter()
            .map(move |compartment| self.mappings[compartment].values())
            .flatten()
    }

    /// Returns whether this source value matched one of the mappings.
    fn control_midi(&mut self, value: MidiSourceValue<RawShortMessage>) -> bool {
        let matched_controller = if let [ref mut controller_mappings, ref main_mappings] =
            self.mappings.as_mut_slice()
        {
            control_midi_virtual_and_reaper_targets(
                &self.control_main_task_sender,
                controller_mappings,
                main_mappings,
                value,
            )
        } else {
            unreachable!()
        };
        let matched_main =
            self.control_midi_reaper_targets(MappingCompartment::MainMappings, value);
        matched_main || matched_controller
    }

    fn control_midi_reaper_targets(
        &mut self,
        compartment: MappingCompartment,
        source_value: MidiSourceValue<RawShortMessage>,
    ) -> bool {
        let mut matched = false;
        for m in self.mappings[compartment]
            .values_mut()
            .filter(|m| m.control_is_effectively_on() && m.has_reaper_target())
        {
            if let Some(control_value) = m
                .source()
                .control(&CompoundMappingSourceValue::Midi(source_value))
            {
                control_main(
                    &self.control_main_task_sender,
                    compartment,
                    m.id(),
                    control_value,
                    ControlOptions {
                        enforce_send_feedback_after_control: false,
                    },
                );
                matched = true;
            }
        }
        matched
    }

    fn process_matched_short(&self, msg: RawShortMessage, caller: Caller) {
        if self.midi_control_input != MidiControlInput::FxInput {
            return;
        }
        if !self.let_matched_events_through {
            return;
        }
        self.send_midi_to_fx_output(msg, caller);
    }

    fn process_unmatched_short(&self, msg: RawShortMessage, caller: Caller) {
        if self.midi_control_input != MidiControlInput::FxInput {
            return;
        }
        if !self.let_unmatched_events_through {
            return;
        }
        self.send_midi_to_fx_output(msg, caller);
    }

    fn is_consumed_by_at_least_one_source(&self, msg: RawShortMessage) -> bool {
        self.all_mappings()
            .any(|m| m.control_is_effectively_on() && m.consumes(msg))
    }

    fn feedback_midi(&self, value: MidiSourceValue<RawShortMessage>, caller: Caller) {
        if let Some(output) = self.midi_feedback_output {
            let shorts = value.to_short_messages(DataEntryByteOrder::MsbFirst);
            if shorts[0].is_none() {
                return;
            }
            match output {
                MidiFeedbackOutput::FxOutput => {
                    for short in shorts.iter().flatten() {
                        self.send_midi_to_fx_output(*short, caller);
                    }
                }
                MidiFeedbackOutput::Device(dev) => {
                    dev.with_midi_output(|mo| {
                        for short in shorts.iter().flatten() {
                            mo.send(*short, SendMidiTime::Instantly);
                        }
                    });
                }
            };
        }
    }

    fn feedback_virtual(&self, value: VirtualSourceValue, caller: Caller) {
        if let ControlValue::Absolute(v) = value.control_value() {
            for m in self
                // Only controller mappings can have virtual targets.
                .mappings[MappingCompartment::ControllerMappings]
                .values()
                .filter(|m| m.feedback_is_effectively_on())
            {
                // TODO-low Mmh, very nested
                if let Some(UnresolvedCompoundMappingTarget::Virtual(t)) = m.target() {
                    if t.control_element() == value.control_element() {
                        if let Some(midi_value) = m.feedback(v) {
                            self.feedback_midi(midi_value, caller);
                        }
                    }
                }
            }
        }
    }

    fn send_midi_to_fx_output(&self, msg: RawShortMessage, caller: Caller) {
        let host = if let Caller::Vst(h) = caller {
            h
        } else {
            // We must not forward MIDI to VST output if this was called from the global audio hook.
            // First, it could lead to strange effects because `HostCallback::process_events()` is
            // supposed to be called only from the VST processing method.
            // Second, it could even lead to a crash because the real-time processor is removed from
            // the audio hook *after* the plug-in has been already unregistered, and then invoking
            // the host callback (in particular dereferencing the AEffect) would be illegal.
            // This is just a last safety check. Ideally, processing should stop before even calling
            // this method.
            panic!("send_midi_to_fx_output() should only be called from VST plug-in");
        };
        let bytes = msg.to_bytes();
        let mut event = MidiEvent {
            event_type: EventType::Midi,
            byte_size: std::mem::size_of::<MidiEvent>() as _,
            delta_frames: 0,
            flags: vst::api::MidiEventFlags::REALTIME_EVENT.bits(),
            note_length: 0,
            note_offset: 0,
            midi_data: [bytes.0, bytes.1.get(), bytes.2.get()],
            _midi_reserved: 0,
            detune: 0,
            note_off_velocity: 0,
            _reserved1: 0,
            _reserved2: 0,
        };
        let events = Events {
            num_events: 1,
            _reserved: 0,
            events: [&mut event as *mut MidiEvent as _, null_mut()],
        };
        host.process_events(&events);
    }
}

#[derive(Copy, Clone)]
enum Caller<'a> {
    Vst(&'a HostCallback),
    AudioHook,
}

/// A task which is sent from time to time.
#[derive(Debug)]
pub enum NormalRealTimeTask {
    UpdateAllMappings(MappingCompartment, Vec<RealTimeMapping>),
    UpdateSingleMapping(MappingCompartment, Box<RealTimeMapping>),
    UpdateSettings {
        let_matched_events_through: bool,
        let_unmatched_events_through: bool,
        midi_control_input: MidiControlInput,
        midi_feedback_output: Option<MidiFeedbackOutput>,
    },
    /// This takes care of propagating target activation states.
    ///
    /// The given set contains *all* mappings whose target is active.
    UpdateTargetActivations(MappingCompartment, HashSet<MappingId>),
    /// Updates the activation state of multiple mappings.
    ///
    /// The given vector contains updates just for affected mappings. This is because when a
    /// parameter update occurs we can determine in a very granular way which targets are affected.
    UpdateMappingActivations(MappingCompartment, Vec<MappingActivationUpdate>),
    LogDebugInfo,
    UpdateSampleRate(Hz),
    StartLearnSource(MappingCompartment),
    DisableControl,
    ReturnToControlMode,
}

#[derive(Copy, Clone, Debug)]
pub struct MappingActivationEffect {
    pub id: MappingId,
    pub active_1_effect: Option<bool>,
    pub active_2_effect: Option<bool>,
}

impl MappingActivationEffect {
    pub fn new(
        id: MappingId,
        active_1_effect: Option<bool>,
        active_2_effect: Option<bool>,
    ) -> Option<MappingActivationEffect> {
        if active_1_effect.is_none() && active_2_effect.is_none() {
            return None;
        }
        let and = MappingActivationEffect {
            id,
            active_1_effect,
            active_2_effect,
        };
        Some(and)
    }
}

#[derive(Copy, Clone, Debug)]
pub struct MappingActivationUpdate {
    pub id: MappingId,
    pub is_active: bool,
}

impl MappingActivationUpdate {
    pub fn new(id: MappingId, is_active: bool) -> MappingActivationUpdate {
        MappingActivationUpdate { id, is_active }
    }
}

/// A feedback task (which is potentially sent very frequently).
#[derive(Debug)]
pub enum FeedbackRealTimeTask {
    // TODO-low Is it better for performance to push a vector (smallvec) here?
    Feedback(CompoundMappingSourceValue),
}

impl Drop for RealTimeProcessor {
    fn drop(&mut self) {
        debug!(self.logger, "Dropping real-time processor...");
    }
}

/// MIDI source which provides ReaLearn control data.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MidiControlInput {
    /// Processes MIDI messages which are fed into ReaLearn FX.
    FxInput,
    /// Processes MIDI messages coming directly from a MIDI input device.
    Device(MidiInputDevice),
}

/// MIDI destination to which ReaLearn's feedback data is sent.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MidiFeedbackOutput {
    /// Routes feedback messages to the ReaLearn FX output.
    FxOutput,
    /// Routes feedback messages directly to a MIDI output device.
    Device(MidiOutputDevice),
}

fn control_midi_virtual_and_reaper_targets(
    sender: &crossbeam_channel::Sender<ControlMainTask>,
    // Controller mappings
    mappings_with_virtual_targets: &mut HashMap<MappingId, RealTimeMapping>,
    // Main mappings
    mappings_with_virtual_sources: &HashMap<MappingId, RealTimeMapping>,
    value: MidiSourceValue<RawShortMessage>,
) -> bool {
    let mut matched = false;
    for m in mappings_with_virtual_targets
        .values_mut()
        .filter(|m| m.control_is_effectively_on())
    {
        if let Some(control_match) = m.control(value) {
            use PartialControlMatch::*;
            let mapping_matched = match control_match {
                ProcessVirtual(virtual_source_value) => control_virtual(
                    sender,
                    mappings_with_virtual_sources,
                    virtual_source_value,
                    ControlOptions {
                        // We inherit "Send feedback after control" to the main processor if it's
                        // enabled for the virtual mapping. That's the easy way to do it.
                        // Downside: If multiple real control elements are mapped to one virtual
                        // control element, "feedback after control" will be sent to all of those,
                        // which is technically not necessary. It would be enough to just send it
                        // to the one that was touched. However, it also doesn't really hurt.
                        enforce_send_feedback_after_control: m
                            .options()
                            .send_feedback_after_control,
                    },
                ),
                ForwardToMain(control_value) => {
                    control_main(
                        sender,
                        MappingCompartment::ControllerMappings,
                        m.id(),
                        control_value,
                        ControlOptions {
                            enforce_send_feedback_after_control: false,
                        },
                    );
                    true
                }
            };
            if mapping_matched {
                matched = true;
            }
        }
    }
    matched
}

fn control_main(
    sender: &crossbeam_channel::Sender<ControlMainTask>,
    compartment: MappingCompartment,
    mapping_id: MappingId,
    value: ControlValue,
    options: ControlOptions,
) {
    let task = ControlMainTask::Control {
        compartment,
        mapping_id,
        value,
        options,
    };
    // If plug-in dropped, the receiver might be gone already because main processor is
    // unregistered synchronously.
    let _ = sender.send(task);
}

/// Returns whether this source value matched one of the mappings.
fn control_virtual(
    sender: &crossbeam_channel::Sender<ControlMainTask>,
    main_mappings: &HashMap<MappingId, RealTimeMapping>,
    value: VirtualSourceValue,
    options: ControlOptions,
) -> bool {
    // Controller mappings can't have virtual sources, so for now we only need to check
    // main mappings.
    let mut matched = false;
    for m in main_mappings
        .values()
        .filter(|m| m.control_is_effectively_on())
    {
        if let Some(control_value) = m
            .source()
            .control(&CompoundMappingSourceValue::Virtual(value))
        {
            control_main(
                sender,
                MappingCompartment::MainMappings,
                m.id(),
                control_value,
                options,
            );
            matched = true;
        }
    }
    matched
}

#[derive(Eq, PartialEq)]
enum Driver {
    AudioHook,
    Vst,
}
