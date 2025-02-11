use crate::application::{CompartmentModel, Preset};
use std::fmt;

#[derive(Clone, Debug)]
pub struct ControllerPreset {
    id: String,
    name: String,
    data: CompartmentModel,
}

impl ControllerPreset {
    pub fn new(id: String, name: String, data: CompartmentModel) -> ControllerPreset {
        ControllerPreset { id, name, data }
    }

    pub fn update_custom_data(&mut self, key: String, value: serde_json::Value) {
        self.data.custom_data.insert(key, value);
    }

    pub fn update_realearn_data(&mut self, data: CompartmentModel) {
        self.data = data;
    }
}

impl Preset for ControllerPreset {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn data(&self) -> &CompartmentModel {
        &self.data
    }
}

impl fmt::Display for ControllerPreset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
