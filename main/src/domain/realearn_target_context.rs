use crate::base::{NamedChannelSender, SenderToNormalThread};
use crate::domain::{
    pot, AdditionalFeedbackEvent, FxSnapshotLoadedEvent, ParameterAutomationTouchStateChangedEvent,
    TouchedTrackParameterType,
};
use reaper_high::{Fx, GroupingBehavior, Track};
use reaper_medium::{GangBehavior, MediaTrack};
use std::collections::{HashMap, HashSet};

/// Feedback for most targets comes from REAPER itself but there are some targets for which ReaLearn
/// holds the state. It's in this struct.
///
/// Some of this state can be persistent. This raises the question which ReaLearn instance should
/// be responsible for saving it. If you need persistent state, first think about if it shouldn't
/// rather be part of `InstanceState`. Then it's owned by a particular instance, which is then also
/// responsible for saving it. But we also have global REAPER things such as additional FX state.
/// In this case, we should put it here and track for each state which instance is responsible for
/// saving it!
pub struct RealearnTargetState {
    /// For notifying ReaLearn about state changes.
    additional_feedback_event_sender: SenderToNormalThread<AdditionalFeedbackEvent>,
    /// Memorizes for each FX the hash of its last FX snapshot loaded via "Load FX snapshot" target.
    ///
    /// Persistent.
    // TODO-high CONTINUE Restore on load (by looking up snapshot chunk)
    fx_snapshot_chunk_hash_by_fx: HashMap<Fx, u64>,
    /// Memorizes for each FX some infos about its last loaded Pot preset.
    ///
    /// Persistent.
    // TODO-high CONTINUE Restore on load (by looking up DB)
    current_pot_preset_by_fx: HashMap<Fx, pot::CurrentPreset>,
    /// Memorizes all currently touched track parameters.
    ///
    /// For "Touch automation state" target.
    ///
    /// Not persistent.
    touched_things: HashSet<TouchedThing>,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
struct TouchedThing {
    track: MediaTrack,
    parameter_type: TouchedTrackParameterType,
}

impl TouchedThing {
    pub fn new(track: MediaTrack, parameter_type: TouchedTrackParameterType) -> Self {
        Self {
            track,
            parameter_type,
        }
    }
}

impl RealearnTargetState {
    pub fn new(
        additional_feedback_event_sender: SenderToNormalThread<AdditionalFeedbackEvent>,
    ) -> Self {
        Self {
            additional_feedback_event_sender,
            fx_snapshot_chunk_hash_by_fx: Default::default(),
            touched_things: Default::default(),
            current_pot_preset_by_fx: Default::default(),
        }
    }

    pub fn current_fx_preset(&self, fx: &Fx) -> Option<&pot::CurrentPreset> {
        self.current_pot_preset_by_fx.get(fx)
    }

    pub fn set_current_fx_preset(&mut self, fx: Fx, current_preset: pot::CurrentPreset) {
        self.current_pot_preset_by_fx.insert(fx, current_preset);
        self.additional_feedback_event_sender
            .send_complaining(AdditionalFeedbackEvent::MappedFxParametersChanged);
    }

    pub fn current_fx_snapshot_chunk_hash(&self, fx: &Fx) -> Option<u64> {
        self.fx_snapshot_chunk_hash_by_fx.get(fx).copied()
    }

    pub fn load_fx_snapshot(
        &mut self,
        fx: Fx,
        chunk: &str,
        chunk_hash: u64,
    ) -> Result<(), &'static str> {
        fx.set_tag_chunk(chunk)?;
        // fx.set_vst_chunk_encoded(chunk.to_string())?;
        self.fx_snapshot_chunk_hash_by_fx
            .insert(fx.clone(), chunk_hash);
        self.additional_feedback_event_sender.send_complaining(
            AdditionalFeedbackEvent::FxSnapshotLoaded(FxSnapshotLoadedEvent { fx }),
        );
        Ok(())
    }

    pub fn touch_automation_parameter(
        &mut self,
        track: &Track,
        parameter_type: TouchedTrackParameterType,
    ) {
        self.touched_things
            .insert(TouchedThing::new(track.raw(), parameter_type));
        self.post_process_touch(track, parameter_type);
        self.additional_feedback_event_sender.send_complaining(
            AdditionalFeedbackEvent::ParameterAutomationTouchStateChanged(
                ParameterAutomationTouchStateChangedEvent {
                    track: track.raw(),
                    parameter_type,
                    new_value: true,
                },
            ),
        );
    }

    pub fn untouch_automation_parameter(
        &mut self,
        track: &Track,
        parameter_type: TouchedTrackParameterType,
    ) {
        self.touched_things
            .remove(&TouchedThing::new(track.raw(), parameter_type));
        self.additional_feedback_event_sender.send_complaining(
            AdditionalFeedbackEvent::ParameterAutomationTouchStateChanged(
                ParameterAutomationTouchStateChangedEvent {
                    track: track.raw(),
                    parameter_type,
                    new_value: false,
                },
            ),
        );
    }

    fn post_process_touch(&mut self, track: &Track, parameter_type: TouchedTrackParameterType) {
        match parameter_type {
            TouchedTrackParameterType::Volume => {
                track.set_volume(
                    track.volume(),
                    GangBehavior::DenyGang,
                    GroupingBehavior::PreventGrouping,
                );
            }
            TouchedTrackParameterType::Pan => {
                track.set_pan(
                    track.pan(),
                    GangBehavior::DenyGang,
                    GroupingBehavior::PreventGrouping,
                );
            }
            TouchedTrackParameterType::Width => {
                track.set_width(
                    track.width(),
                    GangBehavior::DenyGang,
                    GroupingBehavior::PreventGrouping,
                );
            }
        }
    }

    pub fn automation_parameter_is_touched(
        &self,
        track: MediaTrack,
        parameter_type: TouchedTrackParameterType,
    ) -> bool {
        self.touched_things
            .contains(&TouchedThing::new(track, parameter_type))
    }
}
