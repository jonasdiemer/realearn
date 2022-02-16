use crate::buffer::{AudioBufMut, OwnedAudioBuffer};
use crate::supplier::{
    convert_duration_in_frames_to_seconds, convert_duration_in_seconds_to_frames,
    print_distance_from_beat_start_at, AudioSupplier, ExactFrameCount, MidiSupplier,
    SupplyAudioRequest, SupplyMidiRequest, SupplyResponse, WithFrameRate,
};
use crate::{clip_timeline, SupplyRequestInfo, SupplyResponseStatus};
use core::cmp;
use reaper_medium::{
    BorrowedMidiEventList, BorrowedPcmSource, DurationInSeconds, Hz, PcmSourceTransfer,
    PositionInSeconds,
};

#[derive(Debug)]
pub struct Looper<S> {
    loop_behavior: LoopBehavior,
    enabled: bool,
    supplier: S,
}

#[derive(Debug)]
pub enum LoopBehavior {
    Infinitely,
    UntilEndOfCycle(usize),
}

impl Default for LoopBehavior {
    fn default() -> Self {
        Self::UntilEndOfCycle(0)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Repetition {
    Infinitely,
    Once,
}

impl Repetition {
    pub fn from_bool(repeated: bool) -> Self {
        if repeated {
            Repetition::Infinitely
        } else {
            Repetition::Once
        }
    }
}

impl LoopBehavior {
    pub fn from_repetition(repetition: Repetition) -> Self {
        use Repetition::*;
        match repetition {
            Infinitely => Self::Infinitely,
            Once => Self::UntilEndOfCycle(0),
        }
    }

    pub fn from_bool(repeated: bool) -> Self {
        if repeated {
            Self::Infinitely
        } else {
            Self::UntilEndOfCycle(0)
        }
    }

    fn last_cycle(&self) -> Option<usize> {
        use LoopBehavior::*;
        match self {
            Infinitely => None,
            UntilEndOfCycle(n) => Some(*n),
        }
    }
}

impl<S: ExactFrameCount> Looper<S> {
    pub fn new(supplier: S) -> Self {
        Self {
            loop_behavior: Default::default(),
            enabled: false,
            supplier,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn supplier(&self) -> &S {
        &self.supplier
    }

    pub fn supplier_mut(&mut self) -> &mut S {
        &mut self.supplier
    }

    pub fn set_loop_behavior(&mut self, loop_behavior: LoopBehavior) {
        self.loop_behavior = loop_behavior;
    }

    pub fn keep_playing_until_end_of_current_cycle(&mut self, pos: isize) {
        // TODO-high Scheduling for stop after 2nd cycle plays a bit
        //  too far. Check MIDI clip, plays the downbeat!
        let last_cycle = if pos < 0 {
            0
        } else {
            self.get_cycle_at_frame(pos as usize)
        };
        self.loop_behavior = LoopBehavior::UntilEndOfCycle(last_cycle);
    }

    pub fn get_cycle_at_frame(&self, frame: usize) -> usize {
        frame / self.supplier.frame_count()
    }

    fn check_relevance(&self, start_frame: isize) -> Option<RelevantData> {
        if !self.enabled || start_frame < 0 {
            return None;
        }
        let start_frame = start_frame as usize;
        let current_cycle = self.get_cycle_at_frame(start_frame);
        let cycle_in_scope = self
            .loop_behavior
            .last_cycle()
            .map(|last_cycle| current_cycle <= last_cycle)
            .unwrap_or(true);
        if !cycle_in_scope {
            return None;
        }
        let data = RelevantData {
            start_frame,
            current_cycle,
        };
        Some(data)
    }

    fn is_last_cycle(&self, cycle: usize) -> bool {
        self.loop_behavior
            .last_cycle()
            .map(|last_cycle| cycle == last_cycle)
            .unwrap_or(false)
    }
}

struct RelevantData {
    start_frame: usize,
    current_cycle: usize,
}

impl<S: AudioSupplier + ExactFrameCount> AudioSupplier for Looper<S> {
    fn supply_audio(
        &mut self,
        request: &SupplyAudioRequest,
        dest_buffer: &mut AudioBufMut,
    ) -> SupplyResponse {
        let data = match self.check_relevance(request.start_frame) {
            None => {
                return self.supplier.supply_audio(&request, dest_buffer);
            }
            Some(d) => d,
        };
        let start_frame = data.start_frame;
        let supplier_frame_count = self.supplier.frame_count();
        // Start from beginning if we encounter a start frame after the end (modulo).
        let modulo_start_frame = start_frame % supplier_frame_count;
        let modulo_request = SupplyAudioRequest {
            start_frame: modulo_start_frame as isize,
            dest_sample_rate: request.dest_sample_rate,
            info: SupplyRequestInfo {
                audio_block_frame_offset: request.info.audio_block_frame_offset,
                requester: "looper-audio-modulo-request",
                note: "",
            },
            parent_request: Some(request),
            general_info: request.general_info,
        };
        let modulo_response = self.supplier.supply_audio(&modulo_request, dest_buffer);
        match modulo_response.status {
            SupplyResponseStatus::PleaseContinue => modulo_response,
            SupplyResponseStatus::ReachedEnd { num_frames_written } => {
                if self.is_last_cycle(data.current_cycle) {
                    // Time to stop.
                    modulo_response
                } else if num_frames_written == dest_buffer.frame_count() {
                    // Perfect landing, source completely consumed. Start next cycle.
                    SupplyResponse::please_continue(modulo_response.num_frames_consumed)
                } else {
                    // Exceeded end of source.
                    // We need to fill the rest with material from the beginning of the source.
                    let start_request = SupplyAudioRequest {
                        start_frame: 0,
                        dest_sample_rate: request.dest_sample_rate,
                        info: SupplyRequestInfo {
                            audio_block_frame_offset: request.info.audio_block_frame_offset
                                + num_frames_written,
                            requester: "looper-audio-start-request",
                            note: "",
                        },
                        parent_request: Some(request),
                        general_info: request.general_info,
                    };
                    let start_response = self.supplier.supply_audio(
                        &start_request,
                        &mut dest_buffer.slice_mut(num_frames_written..),
                    );
                    SupplyResponse::please_continue(
                        modulo_response.num_frames_consumed + start_response.num_frames_consumed,
                    )
                }
            }
        }
    }

    fn channel_count(&self) -> usize {
        self.supplier.channel_count()
    }
}

impl<S: WithFrameRate> WithFrameRate for Looper<S> {
    fn frame_rate(&self) -> Option<Hz> {
        self.supplier.frame_rate()
    }
}

impl<S: MidiSupplier + ExactFrameCount> MidiSupplier for Looper<S> {
    fn supply_midi(
        &mut self,
        request: &SupplyMidiRequest,
        event_list: &BorrowedMidiEventList,
    ) -> SupplyResponse {
        let data = match self.check_relevance(request.start_frame) {
            None => {
                return self.supplier.supply_midi(&request, event_list);
            }
            Some(d) => d,
        };
        let start_frame = data.start_frame;
        let supplier_frame_count = self.supplier.frame_count();
        // Start from beginning if we encounter a start frame after the end (modulo).
        let modulo_start_frame = start_frame % supplier_frame_count;
        let modulo_request = SupplyMidiRequest {
            start_frame: modulo_start_frame as isize,
            dest_frame_count: request.dest_frame_count,
            dest_sample_rate: request.dest_sample_rate,
            info: SupplyRequestInfo {
                audio_block_frame_offset: request.info.audio_block_frame_offset,
                requester: "looper-midi-modulo-request",
                note: "",
            },
            parent_request: Some(request),
            general_info: request.general_info,
        };
        let modulo_response = self.supplier.supply_midi(&modulo_request, event_list);
        match modulo_response.status {
            SupplyResponseStatus::PleaseContinue => modulo_response,
            SupplyResponseStatus::ReachedEnd { num_frames_written } => {
                if self.is_last_cycle(data.current_cycle) {
                    // Time to stop.
                    modulo_response
                } else if num_frames_written == request.dest_frame_count {
                    // Perfect landing, source completely consumed. Start next cycle.
                    SupplyResponse::please_continue(modulo_response.num_frames_consumed)
                } else {
                    // We need to fill the rest with material from the beginning of the source.
                    // Repeat. Fill rest of buffer with beginning of source.
                    // We need to start from negative position so the frame
                    // offset of the *added* MIDI events is correctly written.
                    // The negative position should be as long as the duration of
                    // samples already written.
                    let start_request = SupplyMidiRequest {
                        start_frame: -(modulo_response.num_frames_consumed as isize),
                        dest_sample_rate: request.dest_sample_rate,
                        dest_frame_count: request.dest_frame_count,
                        info: SupplyRequestInfo {
                            audio_block_frame_offset: request.info.audio_block_frame_offset
                                + num_frames_written,
                            requester: "looper-midi-start-request",
                            note: "",
                        },
                        parent_request: Some(request),
                        general_info: request.general_info,
                    };
                    let start_response = self.supplier.supply_midi(&start_request, event_list);
                    // We don't add modulo_response.num_frames_consumed because that number of
                    // consumed frames is already contained in the number returned in the start
                    // response (because we started at a negative start position).
                    SupplyResponse::please_continue(start_response.num_frames_consumed)
                }
            }
        }
    }
}
