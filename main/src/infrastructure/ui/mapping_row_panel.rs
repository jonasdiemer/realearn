use crate::application::{
    MappingModel, SharedMapping, SharedSession, SourceCategory, TargetCategory, WeakSession,
};
use crate::core::when;
use crate::domain::MappingCompartment;

use crate::core::Global;
use crate::infrastructure::ui::bindings::root;
use crate::infrastructure::ui::bindings::root::{
    ID_MAPPING_ROW_CONTROL_CHECK_BOX, ID_MAPPING_ROW_FEEDBACK_CHECK_BOX,
};
use crate::infrastructure::ui::constants::symbols;
use crate::infrastructure::ui::{IndependentPanelManager, SharedMainState};
use reaper_high::Reaper;
use rx_util::UnitEvent;
use rxrust::prelude::*;
use slog::debug;
use std::cell::{Ref, RefCell};
use std::ops::Deref;
use std::rc::{Rc, Weak};
use swell_ui::{DialogUnits, Point, SharedView, View, ViewContext, Window};

pub type SharedIndependentPanelManager = Rc<RefCell<IndependentPanelManager>>;

/// Panel containing the summary data of one mapping and buttons such as "Remove".
#[derive(Debug)]
pub struct MappingRowPanel {
    view: ViewContext,
    session: WeakSession,
    main_state: SharedMainState,
    row_index: u32,
    // We use virtual scrolling in order to be able to show a large amount of rows without any
    // performance issues. That means there's a fixed number of mapping rows and they just
    // display different mappings depending on the current scroll position. If there are less
    // mappings than the fixed number, some rows remain unused. In this case their mapping is
    // `None`, which will make the row hide itself.
    mapping: RefCell<Option<SharedMapping>>,
    // Fires when a mapping is about to change.
    party_is_over_subject: RefCell<LocalSubject<'static, (), ()>>,
    panel_manager: Weak<RefCell<IndependentPanelManager>>,
}

impl MappingRowPanel {
    pub fn new(
        session: WeakSession,
        row_index: u32,
        panel_manager: Weak<RefCell<IndependentPanelManager>>,
        main_state: SharedMainState,
    ) -> MappingRowPanel {
        MappingRowPanel {
            view: Default::default(),
            session,
            main_state,
            row_index,
            party_is_over_subject: Default::default(),
            mapping: None.into(),
            panel_manager,
        }
    }

    pub fn set_mapping(self: &SharedView<Self>, mapping: Option<SharedMapping>) {
        self.party_is_over_subject.borrow_mut().next(());
        match &mapping {
            None => self.view.require_window().hide(),
            Some(m) => {
                self.view.require_window().show();
                self.invalidate_all_controls(m.borrow().deref());
                self.register_listeners(m.borrow().deref());
            }
        }
        self.mapping.replace(mapping);
    }

    fn invalidate_all_controls(&self, mapping: &MappingModel) {
        self.invalidate_name_label(&mapping);
        self.invalidate_source_label(&mapping);
        self.invalidate_target_label(&mapping);
        self.invalidate_learn_source_button(&mapping);
        self.invalidate_learn_target_button(&mapping);
        self.invalidate_control_check_box(&mapping);
        self.invalidate_feedback_check_box(&mapping);
        self.invalidate_on_indicator(&mapping);
        self.invalidate_button_enabled_states();
    }

    fn invalidate_name_label(&self, mapping: &MappingModel) {
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_GROUP_BOX)
            .set_text(mapping.name.get_ref().as_str());
    }

    fn session(&self) -> SharedSession {
        self.session.upgrade().expect("session gone")
    }

    fn invalidate_source_label(&self, mapping: &MappingModel) {
        let plain_label = mapping.source_model.to_string();
        let rich_label = if mapping.source_model.category.get() == SourceCategory::Virtual {
            let session = self.session();
            let session = session.borrow();
            let controller_mappings = session.mappings(MappingCompartment::ControllerMappings);
            let mappings: Vec<_> = controller_mappings
                .filter(|m| {
                    let m = m.borrow();
                    m.target_model.category.get() == TargetCategory::Virtual
                        && m.target_model.create_control_element()
                            == mapping.source_model.create_control_element()
                })
                .collect();
            if mappings.is_empty() {
                plain_label
            } else {
                let first_mapping = mappings[0].borrow();
                let first_mapping_name = first_mapping.name.get_ref().clone();
                if mappings.len() == 1 {
                    format!("{}\n({})", plain_label, first_mapping_name)
                } else {
                    format!(
                        "{}({} + {})",
                        plain_label,
                        first_mapping_name,
                        mappings.len() - 1
                    )
                }
            }
        } else {
            plain_label
        };
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_SOURCE_LABEL_TEXT)
            .set_text(rich_label);
    }

    fn invalidate_target_label(&self, mapping: &MappingModel) {
        let target_model_string = mapping
            .target_model
            .with_context(self.session().borrow().context())
            .to_string();
        self.view
            .require_window()
            .require_control(root::ID_MAPPING_ROW_TARGET_LABEL_TEXT)
            .set_text(target_model_string);
    }

    fn invalidate_learn_source_button(&self, mapping: &MappingModel) {
        let text = if self.session().borrow().mapping_is_learning_source(mapping) {
            "Stop"
        } else {
            "Learn source"
        };
        self.view
            .require_control(root::ID_MAPPING_ROW_LEARN_SOURCE_BUTTON)
            .set_text(text);
    }

    fn invalidate_learn_target_button(&self, mapping: &MappingModel) {
        let text = if self.session().borrow().mapping_is_learning_target(mapping) {
            "Stop"
        } else {
            "Learn target"
        };
        self.view
            .require_control(root::ID_MAPPING_ROW_LEARN_TARGET_BUTTON)
            .set_text(text);
    }

    fn use_arrow_characters(&self) {
        self.view
            .require_control(root::ID_MAPPING_ROW_CONTROL_CHECK_BOX)
            .set_text(symbols::ARROW_RIGHT_SYMBOL.to_string());
        self.view
            .require_control(root::ID_MAPPING_ROW_FEEDBACK_CHECK_BOX)
            .set_text(symbols::ARROW_LEFT_SYMBOL.to_string());
        self.view
            .require_control(root::ID_UP_BUTTON)
            .set_text(symbols::ARROW_UP_SYMBOL.to_string());
        self.view
            .require_control(root::ID_DOWN_BUTTON)
            .set_text(symbols::ARROW_DOWN_SYMBOL.to_string());
    }

    fn invalidate_control_check_box(&self, mapping: &MappingModel) {
        self.view
            .require_control(root::ID_MAPPING_ROW_CONTROL_CHECK_BOX)
            .set_checked(mapping.control_is_enabled.get());
    }

    fn invalidate_feedback_check_box(&self, mapping: &MappingModel) {
        self.view
            .require_control(root::ID_MAPPING_ROW_FEEDBACK_CHECK_BOX)
            .set_checked(mapping.feedback_is_enabled.get());
    }

    fn invalidate_on_indicator(&self, mapping: &MappingModel) {
        let is_on = self.session().borrow().mapping_is_on(mapping.id());
        self.view
            .require_control(root::ID_MAPPING_ROW_SOURCE_LABEL_TEXT)
            .set_enabled(is_on);
        self.view
            .require_control(root::ID_MAPPING_ROW_TARGET_LABEL_TEXT)
            .set_enabled(is_on);
    }

    fn mappings_are_read_only(&self) -> bool {
        let session = self.session();
        let session = session.borrow();
        session.is_learning_many_mappings()
            || (self.active_compartment() == MappingCompartment::MainMappings
                && session.main_preset_auto_load_is_active())
    }

    fn invalidate_button_enabled_states(&self) {
        let enabled = !self.mappings_are_read_only();
        let buttons = [
            root::ID_UP_BUTTON,
            root::ID_DOWN_BUTTON,
            root::ID_MAPPING_ROW_CONTROL_CHECK_BOX,
            root::ID_MAPPING_ROW_FEEDBACK_CHECK_BOX,
            root::ID_MAPPING_ROW_EDIT_BUTTON,
            root::ID_MAPPING_ROW_DUPLICATE_BUTTON,
            root::ID_MAPPING_ROW_REMOVE_BUTTON,
            root::ID_MAPPING_ROW_LEARN_SOURCE_BUTTON,
            root::ID_MAPPING_ROW_LEARN_TARGET_BUTTON,
        ];
        for b in buttons.into_iter() {
            self.view.require_control(*b).set_enabled(enabled);
        }
    }

    fn register_listeners(self: &SharedView<Self>, mapping: &MappingModel) {
        let session = self.session();
        let session = session.borrow();
        self.when(mapping.name.changed(), |view| {
            view.with_mapping(Self::invalidate_name_label);
        });
        self.when(mapping.source_model.changed(), |view| {
            view.with_mapping(Self::invalidate_source_label);
        });
        self.when(
            mapping
                .target_model
                .changed()
                // We also want to reflect track name changes immediately.
                .merge(Global::control_surface_rx().track_name_changed().map_to(())),
            |view| {
                view.with_mapping(Self::invalidate_target_label);
            },
        );
        self.when(mapping.control_is_enabled.changed(), |view| {
            view.with_mapping(Self::invalidate_control_check_box);
        });
        self.when(mapping.feedback_is_enabled.changed(), |view| {
            view.with_mapping(Self::invalidate_feedback_check_box);
        });
        self.when(session.mapping_which_learns_source_changed(), |view| {
            view.with_mapping(Self::invalidate_learn_source_button);
        });
        self.when(session.mapping_which_learns_target_changed(), |view| {
            view.with_mapping(Self::invalidate_learn_target_button);
        });
        self.when(session.on_mappings_changed(), |view| {
            view.with_mapping(Self::invalidate_on_indicator);
        });
        self.when(
            session
                .main_preset_auto_load_mode
                .changed()
                .merge(session.learn_many_state_changed()),
            |view| {
                view.invalidate_button_enabled_states();
            },
        );
    }

    fn with_mapping(&self, use_mapping: impl Fn(&Self, &MappingModel)) {
        let mapping = self.mapping.borrow();
        if let Some(m) = mapping.as_ref() {
            use_mapping(self, m.borrow().deref())
        }
    }

    fn closed_or_mapping_will_change(&self) -> impl UnitEvent {
        self.view
            .closed()
            .merge(self.party_is_over_subject.borrow().clone())
    }

    fn require_mapping(&self) -> Ref<SharedMapping> {
        Ref::map(self.mapping.borrow(), |m| m.as_ref().unwrap())
    }

    fn require_mapping_address(&self) -> *const MappingModel {
        self.mapping.borrow().as_ref().unwrap().as_ptr()
    }

    fn edit_mapping(&self) {
        self.panel_manager()
            .borrow_mut()
            .edit_mapping(self.require_mapping().deref());
    }

    fn panel_manager(&self) -> SharedIndependentPanelManager {
        self.panel_manager.upgrade().expect("panel manager gone")
    }

    fn move_mapping_up(&self) {
        self.session()
            .borrow_mut()
            .move_mapping_up(self.active_compartment(), self.require_mapping_address());
    }

    fn active_compartment(&self) -> MappingCompartment {
        self.main_state.borrow().active_compartment.get()
    }

    fn move_mapping_down(&self) {
        self.session()
            .borrow_mut()
            .move_mapping_down(self.active_compartment(), self.require_mapping_address());
    }

    fn remove_mapping(&self) {
        if !self
            .view
            .require_window()
            .confirm("ReaLearn", "Do you really want to remove this mapping?")
        {
            return;
        }
        self.session()
            .borrow_mut()
            .remove_mapping(self.active_compartment(), self.require_mapping_address());
    }

    fn duplicate_mapping(&self) {
        self.session()
            .borrow_mut()
            .duplicate_mapping(self.active_compartment(), self.require_mapping_address())
            .unwrap();
    }

    fn toggle_learn_source(&self) {
        let shared_session = self.session();
        shared_session
            .borrow_mut()
            .toggle_learning_source(&shared_session, self.require_mapping().deref());
    }

    fn toggle_learn_target(&self) {
        let shared_session = self.session();
        shared_session
            .borrow_mut()
            .toggle_learning_target(&shared_session, self.require_mapping().deref());
    }

    fn update_control_is_enabled(&self) {
        self.require_mapping().borrow_mut().control_is_enabled.set(
            self.view
                .require_control(ID_MAPPING_ROW_CONTROL_CHECK_BOX)
                .is_checked(),
        );
    }

    fn update_feedback_is_enabled(&self) {
        self.require_mapping().borrow_mut().feedback_is_enabled.set(
            self.view
                .require_control(ID_MAPPING_ROW_FEEDBACK_CHECK_BOX)
                .is_checked(),
        );
    }

    fn when(
        self: &SharedView<Self>,
        event: impl UnitEvent,
        reaction: impl Fn(SharedView<Self>) + 'static + Copy,
    ) {
        when(event.take_until(self.closed_or_mapping_will_change()))
            .with(Rc::downgrade(self))
            .do_sync(move |panel, _| reaction(panel));
    }
}

impl View for MappingRowPanel {
    fn dialog_resource_id(&self) -> u32 {
        root::ID_MAPPING_ROW_PANEL
    }

    fn view_context(&self) -> &ViewContext {
        &self.view
    }

    fn opened(self: SharedView<Self>, window: Window) -> bool {
        window.move_to(Point::new(DialogUnits(0), DialogUnits(self.row_index * 48)));
        self.use_arrow_characters();
        window.hide();
        false
    }

    fn button_clicked(self: SharedView<Self>, resource_id: u32) {
        match resource_id {
            root::ID_MAPPING_ROW_EDIT_BUTTON => self.edit_mapping(),
            root::ID_UP_BUTTON => self.move_mapping_up(),
            root::ID_DOWN_BUTTON => self.move_mapping_down(),
            root::ID_MAPPING_ROW_REMOVE_BUTTON => self.remove_mapping(),
            root::ID_MAPPING_ROW_DUPLICATE_BUTTON => self.duplicate_mapping(),
            root::ID_MAPPING_ROW_LEARN_SOURCE_BUTTON => self.toggle_learn_source(),
            root::ID_MAPPING_ROW_LEARN_TARGET_BUTTON => self.toggle_learn_target(),
            root::ID_MAPPING_ROW_CONTROL_CHECK_BOX => self.update_control_is_enabled(),
            root::ID_MAPPING_ROW_FEEDBACK_CHECK_BOX => self.update_feedback_is_enabled(),
            _ => unreachable!(),
        }
    }
}

impl Drop for MappingRowPanel {
    fn drop(&mut self) {
        debug!(Reaper::get().logger(), "Dropping mapping row panel...");
    }
}
