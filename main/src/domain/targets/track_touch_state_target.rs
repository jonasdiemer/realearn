use crate::domain::{
    change_track_prop, format_value_as_on_off,
    get_control_type_and_character_for_track_exclusivity, get_effective_tracks, touched_unit_value,
    AdditionalFeedbackEvent, BackboneState, Compartment, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, HitResponse, MappingControlContext, RealearnTarget, ReaperTarget,
    ReaperTargetType, TargetCharacter, TargetTypeDef, TrackDescriptor, TrackExclusivity,
    UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{AbsoluteValue, ControlType, ControlValue, Target, UnitValue};
use reaper_high::{Project, Track};
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedTrackTouchStateTarget {
    pub track_descriptor: TrackDescriptor,
    pub parameter_type: TouchedTrackParameterType,
    pub exclusivity: TrackExclusivity,
}

impl UnresolvedReaperTargetDef for UnresolvedTrackTouchStateTarget {
    fn resolve(
        &self,
        context: ExtendedProcessorContext,
        compartment: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(
            get_effective_tracks(context, &self.track_descriptor.track, compartment)?
                .into_iter()
                .map(|track| {
                    ReaperTarget::TrackAutomationTouchState(TrackTouchStateTarget {
                        track,
                        parameter_type: self.parameter_type,
                        exclusivity: self.exclusivity,
                    })
                })
                .collect(),
        )
    }

    fn track_descriptor(&self) -> Option<&TrackDescriptor> {
        Some(&self.track_descriptor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackTouchStateTarget {
    pub track: Track,
    pub parameter_type: TouchedTrackParameterType,
    pub exclusivity: TrackExclusivity,
}

impl RealearnTarget for TrackTouchStateTarget {
    fn control_type_and_character(&self, _: ControlContext) -> (ControlType, TargetCharacter) {
        get_control_type_and_character_for_track_exclusivity(self.exclusivity)
    }

    fn format_value(&self, value: UnitValue, _: ControlContext) -> String {
        format_value_as_on_off(value).to_string()
    }

    fn hit(
        &mut self,
        value: ControlValue,
        _: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let target_context = BackboneState::target_state();
        change_track_prop(
            &self.track,
            self.exclusivity,
            value.to_unit_value()?,
            |t| {
                target_context
                    .borrow_mut()
                    .touch_automation_parameter(t, self.parameter_type)
            },
            |t| {
                target_context
                    .borrow_mut()
                    .untouch_automation_parameter(t, self.parameter_type)
            },
        );
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        self.track.is_available()
    }

    fn project(&self) -> Option<Project> {
        Some(self.track.project())
    }

    fn track(&self) -> Option<&Track> {
        Some(&self.track)
    }

    fn track_exclusivity(&self) -> Option<TrackExclusivity> {
        Some(self.exclusivity)
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        _: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Additional(
                AdditionalFeedbackEvent::ParameterAutomationTouchStateChanged(e),
            ) if e.track == self.track.raw() && e.parameter_type == self.parameter_type => (
                true,
                Some(AbsoluteValue::Continuous(touched_unit_value(e.new_value))),
            ),
            _ => (false, None),
        }
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        Some(format_value_as_on_off(self.current_value(context)?.to_unit_value()).into())
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::TrackTouchState)
    }
}

impl<'a> Target<'a> for TrackTouchStateTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, _: Self::Context) -> Option<AbsoluteValue> {
        let is_touched = BackboneState::target_state()
            .borrow()
            .automation_parameter_is_touched(self.track.raw(), self.parameter_type);
        Some(AbsoluteValue::Continuous(touched_unit_value(is_touched)))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

pub const TRACK_TOUCH_STATE_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Track: Set automation touch state",
    short_name: "Track touch state",
    supports_track: true,
    supports_track_exclusivity: true,
    ..DEFAULT_TARGET
};

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr,
    enum_iterator::IntoEnumIterator,
    num_enum::TryFromPrimitive,
    num_enum::IntoPrimitive,
    derive_more::Display,
)]
#[repr(usize)]
pub enum TouchedTrackParameterType {
    Volume,
    Pan,
    Width,
}

impl Default for TouchedTrackParameterType {
    fn default() -> Self {
        TouchedTrackParameterType::Volume
    }
}

impl TouchedTrackParameterType {
    pub fn try_from_reaper(
        reaper_type: reaper_medium::TouchedParameterType,
    ) -> Result<Self, &'static str> {
        use reaper_medium::TouchedParameterType::*;
        let res = match reaper_type {
            Volume => Self::Volume,
            Pan => Self::Pan,
            Width => Self::Width,
            Unknown(_) => return Err("unknown touch parameter type"),
        };
        Ok(res)
    }
}
