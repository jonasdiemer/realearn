return {
    key = "volume",
    name = "Volume",
    tags = {
        "mix",
        "master"
    },
    group = "faders",
    visible_in_projection = true,
    enabled = true,
    control_enabled = true,
    feedback_enabled = true,
    active = "Always",
    feedback_behavior = "Normal",
    on_activate = "Normal",
    on_deactivate = "Normal",
    source = {
        type = "MidiControlChangeValue",
        channel = 0,
        controller_number = 64,
        character = "Button",
        fourteen_bit = false
    },
    glue = {
        source_interval = {
            0.3,
            0.7
        }
    },
    target = {
        unit = "Percent"
    }
}