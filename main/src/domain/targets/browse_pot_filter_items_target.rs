use crate::domain::pot::nks::FilterItemId;
use crate::domain::pot::{preset_db, FilterItem, RuntimePotUnit};
use crate::domain::{
    convert_count_to_step_size, convert_discrete_to_unit_value_with_none,
    convert_unit_to_discrete_value_with_none, Compartment, CompoundChangeEvent, ControlContext,
    ExtendedProcessorContext, HitResponse, InstanceStateChanged, MappingControlContext,
    PotStateChangedEvent, RealearnTarget, ReaperTarget, ReaperTargetType, TargetCharacter,
    TargetTypeDef, UnresolvedReaperTargetDef, DEFAULT_TARGET,
};
use helgoboss_learn::{
    AbsoluteValue, ControlType, ControlValue, Fraction, NumericValue, PropValue, Target, UnitValue,
};
use realearn_api::persistence::PotFilterItemKind;
use std::borrow::Cow;

#[derive(Debug)]
pub struct UnresolvedBrowsePotFilterItemsTarget {
    pub settings: PotFilterItemsTargetSettings,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct PotFilterItemsTargetSettings {
    pub kind: PotFilterItemKind,
}

impl UnresolvedReaperTargetDef for UnresolvedBrowsePotFilterItemsTarget {
    fn resolve(
        &self,
        _: ExtendedProcessorContext,
        _: Compartment,
    ) -> Result<Vec<ReaperTarget>, &'static str> {
        Ok(vec![ReaperTarget::BrowsePotFilterItems(
            BrowsePotFilterItemsTarget {
                settings: self.settings.clone(),
            },
        )])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowsePotFilterItemsTarget {
    pub settings: PotFilterItemsTargetSettings,
}

impl RealearnTarget for BrowsePotFilterItemsTarget {
    fn control_type_and_character(
        &self,
        context: ControlContext,
    ) -> (ControlType, TargetCharacter) {
        // `+ 1` because "<None>" is also a possible value.
        let mut instance_state = context.instance_state.borrow_mut();
        let pot_unit = match instance_state.pot_unit() {
            Ok(u) => u,
            Err(_) => return (ControlType::AbsoluteContinuous, TargetCharacter::Continuous),
        };
        let count = self.item_count(pot_unit) + 1;
        let atomic_step_size = convert_count_to_step_size(count);
        (
            ControlType::AbsoluteDiscrete {
                atomic_step_size,
                is_retriggerable: false,
            },
            TargetCharacter::Discrete,
        )
    }

    fn parse_as_value(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text, context)
    }

    fn parse_as_step_size(
        &self,
        text: &str,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        self.parse_value_from_discrete_value(text, context)
    }

    fn convert_unit_value_to_discrete_value(
        &self,
        value: UnitValue,
        context: ControlContext,
    ) -> Result<u32, &'static str> {
        let mut instance_state = context.instance_state.borrow_mut();
        let pot_unit = instance_state.pot_unit()?;
        let value = self
            .convert_unit_value_to_item_index(pot_unit, value)
            .map(|i| i + 1)
            .unwrap_or(0);
        Ok(value)
    }

    fn hit(
        &mut self,
        value: ControlValue,
        context: MappingControlContext,
    ) -> Result<HitResponse, &'static str> {
        let mut instance_state = context.control_context.instance_state.borrow_mut();
        let item_id = {
            let pot_unit = instance_state.pot_unit()?;
            let item_index =
                self.convert_unit_value_to_item_index(pot_unit, value.to_unit_value()?);
            match item_index {
                None => None,
                Some(i) => {
                    let id = pot_unit
                        .find_filter_item_id_at_index(self.settings.kind, i)
                        .ok_or("no filter item found for that index")?;
                    Some(id)
                }
            }
        };
        instance_state.set_pot_filter_item_id(self.settings.kind, item_id)?;
        Ok(HitResponse::processed_with_effect())
    }

    fn is_available(&self, _: ControlContext) -> bool {
        preset_db().is_ok()
    }

    fn process_change_event(
        &self,
        evt: CompoundChangeEvent,
        context: ControlContext,
    ) -> (bool, Option<AbsoluteValue>) {
        match evt {
            CompoundChangeEvent::Instance(InstanceStateChanged::PotStateChanged(
                PotStateChangedEvent::FilterItemChanged { kind, id },
            )) if *kind == self.settings.kind => {
                let mut instance_state = context.instance_state.borrow_mut();
                let pot_unit = match instance_state.pot_unit() {
                    Ok(u) => u,
                    Err(_) => return (false, None),
                };
                let value = self.convert_item_id_to_absolute_value(pot_unit, *id);
                (true, Some(value))
            }
            CompoundChangeEvent::Instance(InstanceStateChanged::PotStateChanged(
                PotStateChangedEvent::IndexesRebuilt,
            )) => (true, None),
            _ => (false, None),
        }
    }

    fn convert_discrete_value_to_unit_value(
        &self,
        value: u32,
        context: ControlContext,
    ) -> Result<UnitValue, &'static str> {
        let index = if value == 0 { None } else { Some(value - 1) };
        let mut instance_state = context.instance_state.borrow_mut();
        let pot_unit = instance_state.pot_unit()?;
        let uv = convert_discrete_to_unit_value_with_none(index, self.item_count(pot_unit));
        Ok(uv)
    }

    fn text_value(&self, context: ControlContext) -> Option<Cow<'static, str>> {
        let mut instance_state = context.instance_state.borrow_mut();
        let pot_unit = instance_state.pot_unit().ok()?;
        let item_id = match self.current_item_id(pot_unit) {
            None => return Some("All".into()),
            Some(id) => id,
        };
        let item = match self.find_item_by_id(pot_unit, item_id) {
            None => return Some("<Not found>".into()),
            Some(p) => p,
        };
        Some(item.name.into())
    }

    fn numeric_value(&self, context: ControlContext) -> Option<NumericValue> {
        let mut instance_state = context.instance_state.borrow_mut();
        let pot_unit = instance_state.pot_unit().ok()?;
        let item_id = self.current_item_id(pot_unit)?;
        let item_index = self.find_index_of_item(pot_unit, item_id)?;
        Some(NumericValue::Discrete(item_index as i32 + 1))
    }

    fn reaper_target_type(&self) -> Option<ReaperTargetType> {
        Some(ReaperTargetType::BrowsePotFilterItems)
    }

    fn prop_value(&self, key: &str, context: ControlContext) -> Option<PropValue> {
        let mut instance_state = context.instance_state.borrow_mut();
        let pot_unit = instance_state.pot_unit().ok()?;
        let item_id = self.current_item_id(pot_unit)?;
        let item = self.find_item_by_id(pot_unit, item_id)?;
        match key {
            "item.parent.name" => {
                if item.parent_name.is_empty() {
                    return None;
                }
                Some(PropValue::Text(item.parent_name.into()))
            }
            "item.name" => Some(PropValue::Text(item.name.into())),
            _ => None,
        }
    }
}

impl<'a> Target<'a> for BrowsePotFilterItemsTarget {
    type Context = ControlContext<'a>;

    fn current_value(&self, context: Self::Context) -> Option<AbsoluteValue> {
        let mut instance_state = context.instance_state.borrow_mut();
        let pot_unit = instance_state.pot_unit().ok()?;
        let item_id = self.current_item_id(pot_unit);
        Some(self.convert_item_id_to_absolute_value(pot_unit, item_id))
    }

    fn control_type(&self, context: Self::Context) -> ControlType {
        self.control_type_and_character(context).0
    }
}

impl BrowsePotFilterItemsTarget {
    fn convert_item_id_to_absolute_value(
        &self,
        pot_unit: &RuntimePotUnit,
        item_id: Option<FilterItemId>,
    ) -> AbsoluteValue {
        let item_index = item_id.and_then(|id| self.find_index_of_item(pot_unit, id));
        let actual = match item_index {
            None => 0,
            Some(i) => i + 1,
        };
        let max = self.item_count(pot_unit);
        AbsoluteValue::Discrete(Fraction::new(actual, max))
    }

    fn item_count(&self, pot_unit: &RuntimePotUnit) -> u32 {
        pot_unit.count_filter_items(self.settings.kind)
    }

    fn convert_unit_value_to_item_index(
        &self,
        pot_unit: &RuntimePotUnit,
        value: UnitValue,
    ) -> Option<u32> {
        convert_unit_to_discrete_value_with_none(value, self.item_count(pot_unit))
    }

    fn current_item_id(&self, pot_unit: &RuntimePotUnit) -> Option<FilterItemId> {
        pot_unit.filter_item_id(self.settings.kind)
    }

    fn find_item_by_id(&self, pot_unit: &RuntimePotUnit, id: FilterItemId) -> Option<FilterItem> {
        pot_unit
            .find_filter_item_by_id(self.settings.kind, id)
            .cloned()
    }

    fn find_index_of_item(&self, pot_unit: &RuntimePotUnit, id: FilterItemId) -> Option<u32> {
        pot_unit.find_index_of_filter_item(self.settings.kind, id)
    }
}

pub const BROWSE_POT_FILTER_ITEMS_TARGET: TargetTypeDef = TargetTypeDef {
    name: "Pot: Browse filter items",
    short_name: "Browse Pot filter items",
    hint: "Highly experimental!!!",
    ..DEFAULT_TARGET
};
