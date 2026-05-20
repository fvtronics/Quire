use super::ui::{
    dim_tile_label, file_subtitle, file_title, icon_button, open_pdf_files, preview_tile,
    rotated_list_preview_prefix, save_pdf_file, tile_controls, tile_label, tile_preview_widget,
};
use super::workspace::{open_output, parent_window, show_toast, update_shell_view_mode};
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::glib;
use std::cell::Cell;
use std::path::{Path, PathBuf};

mod imp {
    use super::super::state::MergeState;
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/fvtronics/folios/merge-workspace.ui")]
    pub struct MergeWorkspace {
        #[template_child]
        pub add_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub empty_add_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub file_count_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub empty_status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub merge_view_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub file_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub merge_file_grid: TemplateChild<gtk::FlowBox>,
        #[template_child]
        pub clear_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub merge_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub open_output_button: TemplateChild<gtk::Button>,

        pub merge: MergeState,
        pub is_running: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MergeWorkspace {
        const NAME: &'static str = "MergeWorkspace";
        type Type = super::MergeWorkspace;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MergeWorkspace {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_callbacks();
            obj.update_view();
        }
    }
    impl WidgetImpl for MergeWorkspace {}
    impl BoxImpl for MergeWorkspace {}
}

glib::wrapper! {
    pub struct MergeWorkspace(ObjectSubclass<imp::MergeWorkspace>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl MergeWorkspace {
    fn setup_callbacks(&self) {
        let imp = self.imp();

        let workspace = self.clone();
        imp.add_button.connect_clicked(move |_| {
            workspace.choose_pdf_files();
        });

        let workspace = self.clone();
        imp.empty_add_button.connect_clicked(move |_| {
            workspace.choose_pdf_files();
        });

        let workspace = self.clone();
        imp.clear_button.connect_clicked(move |_| {
            workspace.imp().merge.clear();
            workspace.update_view();
        });

        let workspace = self.clone();
        imp.merge_button.connect_clicked(move |_| {
            workspace.choose_output_file();
        });

        let workspace = self.clone();
        imp.open_output_button.connect_clicked(move |_| {
            workspace.open_last_output();
        });
    }

    fn choose_pdf_files(&self) {
        let Some(parent) = parent_window(self) else {
            return;
        };
        let workspace = self.clone();
        glib::spawn_future_local(async move {
            let paths = open_pdf_files(&parent, &gettext("Add PDFs"), &gettext("Add")).await;
            if !paths.is_empty() {
                workspace.add_files(paths);
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
                &gettext("Save Merged PDF"),
                &gettext("Merge"),
                "Merged.pdf",
            )
            .await
            {
                workspace.merge_to(path);
            }
        });
    }

    fn add_files(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }

        let imp = self.imp();
        let paths_to_preview = imp.merge.paths_needing_previews(&paths);
        imp.merge.clear_last_output();

        if paths_to_preview.is_empty() {
            imp.merge.add_files(paths);
            self.update_view();
            return;
        }

        imp.merge.begin_loading();
        self.update_view();
        self.load_merge_previews(paths, paths_to_preview);
    }

    fn load_merge_previews(&self, paths: Vec<PathBuf>, paths_to_preview: Vec<PathBuf>) {
        let workspace = self.clone();
        glib::spawn_future_local(async move {
            let mut loaded_paths = Vec::new();
            let mut loaded_previews = Vec::new();

            for path in &paths_to_preview {
                match crate::preview::render_first_page_preview(path.clone()).await {
                    Ok(preview) => {
                        loaded_paths.push(path.clone());
                        if let Some(preview) = preview {
                            loaded_previews.push((path.clone(), preview));
                        }
                    }
                    Err(error) => {
                        show_toast(&workspace, &error.to_string());
                    }
                }
            }

            let files_to_add = paths
                .into_iter()
                .filter(|path| !paths_to_preview.contains(path) || loaded_paths.contains(path))
                .collect::<Vec<_>>();
            workspace
                .imp()
                .merge
                .finish_loading(files_to_add, loaded_previews);
            workspace.update_view();
        });
    }

    fn merge_to(&self, output_file: PathBuf) {
        let imp = self.imp();
        let input_files = imp.merge.pdf_inputs();

        imp.is_running.set(true);
        imp.merge.clear_last_output();
        self.update_view();

        let workspace = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::pdf::merge_pdfs(input_files, output_file).await;
            let imp = workspace.imp();
            imp.is_running.set(false);

            match result {
                Ok(path) => {
                    imp.merge.set_last_output(path);
                    show_toast(&workspace, &gettext("Merged PDF saved"));
                }
                Err(error) => {
                    show_toast(&workspace, &error.to_string());
                }
            }

            workspace.update_view();
        });
    }

    pub(super) fn supports_view_mode(&self) -> bool {
        true
    }

    pub(super) fn has_view_mode_content(&self) -> bool {
        !self.imp().merge.files.borrow().is_empty()
    }

    pub(super) fn set_view_mode(&self, view_mode: super::ViewMode) {
        self.imp()
            .merge_view_stack
            .set_visible_child_name(view_mode.name());
    }

    pub(super) fn update_view(&self) {
        let imp = self.imp();
        let files = imp.merge.files.borrow();
        let has_files = !files.is_empty();
        let is_busy = imp.merge.is_busy(imp.is_running.get());
        let can_merge = files.len() > 1 && !is_busy;
        let previews = imp.merge.previews.borrow();
        let rotations = imp.merge.rotations.borrow();

        imp.file_list.remove_all();
        imp.merge_file_grid.remove_all();
        for (index, path) in files.iter().enumerate() {
            let rotation = *rotations.get(path).unwrap_or(&0);
            imp.file_list.append(&self.file_row(
                index,
                path,
                files.len(),
                previews.get(path),
                rotation,
            ));
            imp.merge_file_grid.append(&self.file_tile(
                index,
                path,
                files.len(),
                previews.get(path),
                rotation,
            ));
        }

        imp.empty_status.set_visible(!has_files);
        imp.merge_view_stack.set_visible(has_files);
        imp.add_button.set_visible(has_files);
        imp.clear_button.set_visible(has_files);
        imp.merge_button.set_visible(has_files);
        imp.open_output_button
            .set_visible(imp.merge.last_output.borrow().is_some());

        imp.add_button.set_sensitive(has_files && !is_busy);
        imp.empty_add_button.set_sensitive(!is_busy);
        imp.clear_button.set_sensitive(has_files && !is_busy);
        imp.merge_button.set_sensitive(can_merge);
        imp.open_output_button
            .set_sensitive(imp.merge.last_output.borrow().is_some() && !is_busy);

        let count_text = if imp.is_running.get() {
            gettext("Merging PDFs...")
        } else if imp.merge.is_loading.get() {
            gettext("Loading PDFs...")
        } else {
            match files.len() {
                0 => gettext("No files selected"),
                count => ngettext("1 PDF selected", "{} PDFs selected", count as u32)
                    .replace("{}", &count.to_string()),
            }
        };
        imp.file_count_label.set_label(&count_text);
        update_shell_view_mode(self);
    }

    fn file_row(
        &self,
        index: usize,
        path: &Path,
        count: usize,
        preview: Option<&crate::preview::PagePreview>,
        rotation: i64,
    ) -> adw::ActionRow {
        let row = adw::ActionRow::builder()
            .title(file_title(path))
            .subtitle(file_subtitle(path))
            .activatable(false)
            .build();

        row.add_prefix(&rotated_list_preview_prefix(preview, rotation));

        let imp = self.imp();
        let controls_sensitive = !imp.merge.is_busy(imp.is_running.get());
        let up_button = icon_button("go-up-symbolic", &gettext("Move Up"));
        up_button.set_sensitive(controls_sensitive && index > 0);
        let window = self.clone();
        up_button.connect_clicked(move |_| {
            window.move_file(index, index - 1);
        });
        row.add_suffix(&up_button);

        let down_button = icon_button("go-down-symbolic", &gettext("Move Down"));
        down_button.set_sensitive(controls_sensitive && index + 1 < count);
        let window = self.clone();
        down_button.connect_clicked(move |_| {
            window.move_file(index, index + 1);
        });
        row.add_suffix(&down_button);

        let rotate_button =
            icon_button("object-rotate-right-symbolic", &gettext("Rotate Clockwise"));
        rotate_button.set_sensitive(controls_sensitive);
        let window = self.clone();
        rotate_button.connect_clicked(move |_| {
            window.rotate_file(index);
        });
        row.add_suffix(&rotate_button);

        let remove_button = icon_button("edit-delete-symbolic", &gettext("Remove"));
        remove_button.set_sensitive(controls_sensitive);
        let window = self.clone();
        remove_button.connect_clicked(move |_| {
            window.remove_file(index);
        });
        row.add_suffix(&remove_button);

        self.add_file_drag_and_drop(&row, index);

        row
    }

    fn file_tile(
        &self,
        index: usize,
        path: &Path,
        count: usize,
        preview: Option<&crate::preview::PagePreview>,
        rotation: i64,
    ) -> gtk::Box {
        let tile = preview_tile();
        tile.append(&tile_preview_widget(preview, rotation));
        tile.append(&tile_label(file_title(path)));

        let controls = tile_controls();
        let size = dim_tile_label(file_subtitle(path));
        controls.append(&size);

        let imp = self.imp();
        let controls_sensitive = !imp.merge.is_busy(imp.is_running.get());
        let up_button = icon_button("go-up-symbolic", &gettext("Move Up"));
        up_button.set_sensitive(controls_sensitive && index > 0);
        let window = self.clone();
        up_button.connect_clicked(move |_| {
            window.move_file(index, index - 1);
        });
        controls.append(&up_button);

        let down_button = icon_button("go-down-symbolic", &gettext("Move Down"));
        down_button.set_sensitive(controls_sensitive && index + 1 < count);
        let window = self.clone();
        down_button.connect_clicked(move |_| {
            window.move_file(index, index + 1);
        });
        controls.append(&down_button);

        let rotate_button =
            icon_button("object-rotate-right-symbolic", &gettext("Rotate Clockwise"));
        rotate_button.set_sensitive(controls_sensitive);
        let window = self.clone();
        rotate_button.connect_clicked(move |_| {
            window.rotate_file(index);
        });
        controls.append(&rotate_button);

        let remove_button = icon_button("edit-delete-symbolic", &gettext("Remove"));
        remove_button.set_sensitive(controls_sensitive);
        let window = self.clone();
        remove_button.connect_clicked(move |_| {
            window.remove_file(index);
        });
        controls.append(&remove_button);

        tile.append(&controls);
        self.add_file_drag_and_drop(&tile, index);

        tile
    }

    fn move_file(&self, from: usize, to: usize) {
        self.imp().merge.move_file(from, to);
        self.update_view();
    }

    fn rotate_file(&self, index: usize) {
        if self.imp().merge.rotate_file(index) {
            self.update_view();
        }
    }

    fn reorder_file(&self, from: usize, to: usize) {
        if self.imp().merge.reorder_file(from, to) {
            self.update_view();
        }
    }

    fn add_file_drag_and_drop(&self, widget: &impl IsA<gtk::Widget>, index: usize) {
        let drag_source = gtk::DragSource::builder()
            .actions(gtk::gdk::DragAction::MOVE)
            .build();
        drag_source.connect_prepare(move |_, _, _| {
            Some(gtk::gdk::ContentProvider::for_value(
                &(index as u32).to_value(),
            ))
        });
        widget.add_controller(drag_source);

        let drop_target = gtk::DropTarget::new(u32::static_type(), gtk::gdk::DragAction::MOVE);
        let window = self.clone();
        drop_target.connect_drop(move |_, value, _, _| {
            let Ok(from) = value.get::<u32>() else {
                return false;
            };

            window.reorder_file(from as usize, index);
            true
        });
        widget.add_controller(drop_target);
    }

    fn remove_file(&self, index: usize) {
        self.imp().merge.remove_file(index);
        self.update_view();
    }

    fn open_last_output(&self) {
        if let Some(path) = self.imp().merge.last_output.borrow().as_ref() {
            open_output(self, path);
        }
    }
}
