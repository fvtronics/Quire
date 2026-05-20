use super::ui::{
    dim_tile_label, open_pdf_file, page_count_label, preview_tile, rotated_list_preview_prefix,
    save_pdf_file, tile_controls, tile_label, tile_preview_widget,
};
use super::workspace::{
    open_output, ordered_item_controls, parent_window, run_output_job, show_preview_error,
    update_shell_view_mode, OrderedItemControlOptions,
};
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use std::cell::Cell;
use std::path::PathBuf;

mod imp {
    use super::super::state::OrganizeState;
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/fvtronics/folios/organize-workspace.ui")]
    pub struct OrganizeWorkspace {
        #[template_child]
        pub organize_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub organize_empty_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub organize_detail_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub organize_empty_status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub organize_view_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub organize_page_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub organize_page_grid: TemplateChild<gtk::FlowBox>,
        #[template_child]
        pub organize_reset_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub organize_save_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub organize_open_output_button: TemplateChild<gtk::Button>,

        pub organize: OrganizeState,
        pub is_running: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for OrganizeWorkspace {
        const NAME: &'static str = "OrganizeWorkspace";
        type Type = super::OrganizeWorkspace;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for OrganizeWorkspace {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_callbacks();
            obj.update_view();
        }
    }
    impl WidgetImpl for OrganizeWorkspace {}
    impl BoxImpl for OrganizeWorkspace {}
}

glib::wrapper! {
    pub struct OrganizeWorkspace(ObjectSubclass<imp::OrganizeWorkspace>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl OrganizeWorkspace {
    fn setup_callbacks(&self) {
        let imp = self.imp();

        let workspace = self.clone();
        imp.organize_choose_button.connect_clicked(move |_| {
            workspace.choose_file();
        });

        let workspace = self.clone();
        imp.organize_empty_choose_button.connect_clicked(move |_| {
            workspace.choose_file();
        });

        let workspace = self.clone();
        imp.organize_reset_button.connect_clicked(move |_| {
            workspace.reset_pdf();
        });

        let workspace = self.clone();
        imp.organize_save_button.connect_clicked(move |_| {
            workspace.choose_output_file();
        });

        let workspace = self.clone();
        imp.organize_open_output_button.connect_clicked(move |_| {
            workspace.open_last_output();
        });
    }

    fn choose_file(&self) {
        let Some(parent) = parent_window(self) else {
            return;
        };
        let workspace = self.clone();
        glib::spawn_future_local(async move {
            if let Some(path) = open_pdf_file(&parent, &gettext("Open PDF"), &gettext("Open")).await
            {
                workspace.load_pdf(path);
            }
        });
    }

    fn choose_output_file(&self) {
        let Some(parent) = parent_window(self) else {
            return;
        };
        let workspace = self.clone();
        glib::spawn_future_local(async move {
            if let Some(path) = save_pdf_file(
                &parent,
                &gettext("Save Organized PDF"),
                &gettext("Save"),
                "Organized.pdf",
            )
            .await
            {
                workspace.organize_to(path);
            }
        });
    }

    fn load_pdf(&self, path: PathBuf) {
        let imp = self.imp();
        imp.organize.begin_loading();
        self.update_view();

        let workspace = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::preview::render_page_previews(path.clone()).await;
            let imp = workspace.imp();

            match result {
                Ok(previews) => {
                    imp.organize.load_document(path, previews);
                }
                Err(error) => {
                    imp.organize.finish_loading_failed();
                    show_preview_error(&workspace, &error);
                }
            }

            workspace.update_view();
        });
    }

    fn reset_pdf(&self) {
        if self.imp().organize.reset() {
            self.update_view();
        }
    }

    fn organize_to(&self, output_file: PathBuf) {
        let imp = self.imp();
        let Some((input_file, page_order)) = imp.organize.selections() else {
            return;
        };

        run_output_job(
            self.clone(),
            crate::pdf::organize_pdf(input_file, page_order, output_file),
            gettext("Organized PDF saved"),
            |workspace, running| workspace.imp().is_running.set(running),
            |workspace| workspace.imp().organize.clear_last_output(),
            |workspace, path| workspace.imp().organize.set_last_output(path),
            Self::update_view,
        );
    }

    pub(super) fn supports_view_mode(&self) -> bool {
        true
    }

    pub(super) fn has_view_mode_content(&self) -> bool {
        self.imp().organize.file.borrow().is_some()
    }

    pub(super) fn set_view_mode(&self, view_mode: super::ViewMode) {
        self.imp()
            .organize_view_stack
            .set_visible_child_name(view_mode.name());
    }

    pub(super) fn update_view(&self) {
        let imp = self.imp();
        let page_order = imp.organize.page_order.borrow();
        let has_file = imp.organize.file.borrow().is_some();
        let has_pages = !page_order.is_empty();
        let is_busy = imp.organize.is_busy(imp.is_running.get());
        let previews = imp.organize.previews.borrow();
        let rotations = imp.organize.rotations.borrow();

        imp.organize_page_list.remove_all();
        imp.organize_page_grid.remove_all();
        for (index, page_number) in page_order.iter().enumerate() {
            let preview = previews.get(page_number);
            let rotation = *rotations.get(page_number).unwrap_or(&0);
            imp.organize_page_list.append(&self.page_row(
                index,
                *page_number,
                page_order.len(),
                preview,
                rotation,
            ));
            imp.organize_page_grid.append(&self.organize_page_tile(
                *page_number,
                preview,
                index,
                page_order.len(),
                rotation,
            ));
        }

        imp.organize_empty_status.set_visible(!has_file);
        imp.organize_view_stack.set_visible(has_file);
        imp.organize_choose_button.set_visible(has_file);
        imp.organize_reset_button.set_visible(has_file);
        imp.organize_save_button.set_visible(has_file);
        imp.organize_open_output_button
            .set_visible(imp.organize.last_output.borrow().is_some());

        imp.organize_choose_button.set_sensitive(!is_busy);
        imp.organize_empty_choose_button.set_sensitive(!is_busy);
        imp.organize_reset_button
            .set_sensitive(has_file && !is_busy);
        imp.organize_save_button
            .set_sensitive(has_pages && !is_busy);
        imp.organize_open_output_button
            .set_sensitive(imp.organize.last_output.borrow().is_some() && !is_busy);

        let detail = if imp.is_running.get() {
            gettext("Organizing pages...")
        } else if imp.organize.is_loading.get() {
            gettext("Loading PDF...")
        } else if has_file {
            page_count_label(page_order.len())
        } else {
            gettext("No PDF selected")
        };
        imp.organize_detail_label.set_label(&detail);
        update_shell_view_mode(self);
    }

    fn page_row(
        &self,
        index: usize,
        page_number: u32,
        count: usize,
        preview: Option<&crate::preview::PagePreview>,
        rotation: i64,
    ) -> adw::ActionRow {
        let row = adw::ActionRow::builder()
            .title(format!("{} {page_number}", gettext("Page")))
            .subtitle(format!(
                "{} {} {} {count}",
                gettext("Position"),
                index + 1,
                gettext("of")
            ))
            .activatable(false)
            .build();

        row.add_prefix(&rotated_list_preview_prefix(preview, rotation));

        let controls_sensitive = !self.imp().is_running.get();
        let workspace = self.clone();
        let move_up = move || workspace.move_page(index, index - 1);
        let workspace = self.clone();
        let move_down = move || workspace.move_page(index, index + 1);
        let workspace = self.clone();
        let rotate = move || workspace.rotate_page(page_number);
        let workspace = self.clone();
        let remove = move || workspace.remove_page(index);
        ordered_item_controls(
            OrderedItemControlOptions {
                controls_sensitive,
                can_move_up: index > 0,
                can_move_down: index + 1 < count,
                can_remove: count > 1,
            },
            move_up,
            move_down,
            rotate,
            remove,
        )
        .append_to_row(&row);

        self.add_page_drag_and_drop(&row, page_number);

        row
    }

    fn organize_page_tile(
        &self,
        page_number: u32,
        preview: Option<&crate::preview::PagePreview>,
        index: usize,
        count: usize,
        rotation: i64,
    ) -> gtk::Box {
        let tile = preview_tile();
        tile.append(&tile_preview_widget(preview, rotation));
        tile.append(&tile_label(format!("{} {}", gettext("Page"), page_number)));

        let controls = tile_controls();
        let position = dim_tile_label(format!("{}/{}", index + 1, count));
        controls.append(&position);

        let controls_sensitive = !self.imp().is_running.get();
        let workspace = self.clone();
        let move_up = move || workspace.move_page(index, index - 1);
        let workspace = self.clone();
        let move_down = move || workspace.move_page(index, index + 1);
        let workspace = self.clone();
        let rotate = move || workspace.rotate_page(page_number);
        let workspace = self.clone();
        let remove = move || workspace.remove_page(index);
        ordered_item_controls(
            OrderedItemControlOptions {
                controls_sensitive,
                can_move_up: index > 0,
                can_move_down: index + 1 < count,
                can_remove: count > 1,
            },
            move_up,
            move_down,
            rotate,
            remove,
        )
        .append_to_box(&controls);

        tile.append(&controls);

        self.add_page_drag_and_drop(&tile, page_number);

        tile
    }

    fn move_page(&self, from: usize, to: usize) {
        self.imp().organize.move_page(from, to);
        self.update_view();
    }

    fn rotate_page(&self, page_number: u32) {
        self.imp().organize.rotate_page(page_number);
        self.update_view();
    }

    fn reorder_page(&self, dragged_page: u32, target_page: u32) {
        if self.imp().organize.reorder_page(dragged_page, target_page) {
            self.update_view();
        }
    }

    fn add_page_drag_and_drop(&self, widget: &impl IsA<gtk::Widget>, page_number: u32) {
        let drag_source = gtk::DragSource::builder()
            .actions(gtk::gdk::DragAction::MOVE)
            .build();
        drag_source.connect_prepare(move |_, _, _| {
            Some(gtk::gdk::ContentProvider::for_value(
                &page_number.to_value(),
            ))
        });
        widget.add_controller(drag_source);

        let drop_target = gtk::DropTarget::new(u32::static_type(), gtk::gdk::DragAction::MOVE);
        let window = self.clone();
        drop_target.connect_drop(move |_, value, _, _| {
            let Ok(dragged_page) = value.get::<u32>() else {
                return false;
            };

            window.reorder_page(dragged_page, page_number);
            true
        });
        widget.add_controller(drop_target);
    }

    fn remove_page(&self, index: usize) {
        if self.imp().organize.remove_page(index) {
            self.update_view();
        }
    }

    fn open_last_output(&self) {
        if let Some(path) = self.imp().organize.last_output.borrow().as_ref() {
            open_output(self, path);
        }
    }
}
