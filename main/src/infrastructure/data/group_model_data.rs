use crate::application::{GroupModel, GroupPropVal, Session};
use crate::base::default_util::is_default;
use crate::domain::{GroupId, GroupKey, MappingCompartment, Tag};
use crate::infrastructure::data::{ActivationConditionData, EnabledData};
use serde::{Deserialize, Serialize};
use std::borrow::BorrowMut;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupModelData {
    /// Doesn't have to be a UUID since 2.11.0-pre.13 and corresponds to the model *key* instead!
    /// Because default group UUID is the default, it won't be serialized.
    #[serde(default, skip_serializing_if = "is_default")]
    pub id: GroupKey,
    /// Saved only in some ReaLearn 2.11.0-pre-releases. Later we persist this in "id" field again.
    /// So this is just for being compatible with those few pre-releases!
    #[serde(default, skip_serializing_if = "is_default")]
    pub key: Option<GroupKey>,
    // Because default group name is empty, it won't be serialized.
    #[serde(default, skip_serializing_if = "is_default")]
    pub name: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub tags: Vec<Tag>,
    #[serde(flatten)]
    pub enabled_data: EnabledData,
    #[serde(flatten)]
    pub activation_condition_data: ActivationConditionData,
}

impl GroupModelData {
    pub fn from_model(model: &GroupModel) -> GroupModelData {
        GroupModelData {
            id: model.key().clone(),
            key: None,
            name: model.name().to_owned(),
            tags: model.tags().to_owned(),
            enabled_data: EnabledData {
                control_is_enabled: model.control_is_enabled(),
                feedback_is_enabled: model.feedback_is_enabled(),
            },
            activation_condition_data: ActivationConditionData::from_model(
                &model.activation_condition_model(),
            ),
        }
    }

    // TODO-medium At the moment, it doesn't make sense to take the session here because
    //  we never set this data directly in the session! However, the interface of the
    //  contained ActivationModelData needs the session.
    pub fn to_model(
        &self,
        session: &mut Session,
        compartment: MappingCompartment,
        is_default_group: bool,
    ) -> GroupModel {
        let mut model = GroupModel::new_from_data(
            compartment,
            if is_default_group {
                GroupId::default()
            } else {
                GroupId::random()
            },
            if is_default_group {
                GroupKey::default()
            } else {
                self.key.clone().unwrap_or_else(|| self.id.clone())
            },
        );
        self.apply_to_model(session, |_, val| {
            // We never need to set with notification for groups. They don't have a reactive UI yet
            // and also can't be real-time-pasted.
            model.set(val);
        });
        model
    }

    fn apply_to_model(
        &self,
        session: &mut Session,
        mut set: impl FnMut(&mut Session, GroupPropVal),
    ) {
        set(session, GroupPropVal::Name(self.name.clone()));
        set(session, GroupPropVal::Tags(self.tags.clone()));
        set(
            session,
            GroupPropVal::ControlIsEnabled(self.enabled_data.control_is_enabled),
        );
        set(
            session,
            GroupPropVal::FeedbackIsEnabled(self.enabled_data.feedback_is_enabled),
        );
        self.activation_condition_data
            .apply_to_model(session, |session, val| {
                set(session, GroupPropVal::ActivationConditionProp(val))
            });
    }
}
