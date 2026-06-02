use super::ui::{
    dim_tile_label, file_subtitle, file_title, list_preview_widget, open_pdf_files, preview_tile,
    save_pdf_file, tile_controls, tile_label, tile_preview_widget,
};
use super::workspace::{
    add_item_context_menu, collection_scroll_position, flow_box_item, load_processable_pdf,
    open_output, ordered_item_context_menu_items, ordered_item_controls, output_option_callback,
    parent_window, preserve_collection_scroll_position, replace_collection_item,
    restore_collection_scroll_position, run_output_job, setup_advanced_options_menu,
    show_pdf_load_error, update_shell_title, update_shell_view_mode, AdvancedOptionsMenu,
    CollectionScrollPosition, ContextMenuItem, OrderedItemActions, OrderedItemControlOptions,
    PdfLoadResult, PendingUndo,
};
use super::PdfTool;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::glib;
use std::path::{Path, PathBuf};

mod imp {
    use super::super::state::MergeState;
    use super::PendingUndo;
    use adw::subclass::prelude::*;
    use gtk::{glib, TemplateChild};
    use std::cell::Cell;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/fvtronics/folios/merge-workspace.ui")]
    pub struct MergeWorkspace {
        #[template_child]
        pub add_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub empty_add_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub merge_actions: TemplateChild<gtk::Box>,
        #[template_child]
        pub empty_status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub merge_view_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub file_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub merge_file_grid: TemplateChild<gtk::FlowBox>,
        #[template_child]
        pub merge_list_scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub merge_grid_scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub clear_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub advanced_options_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub merge_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub open_output_button: TemplateChild<gtk::Button>,

        pub merge: MergeState,
        pub(super) pending_undo: PendingUndo,
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
            obj.refresh_view_state();
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
        let rotate_all = move || workspace.rotate_all_files();
        let modern_pdf = output_option_callback(
            self.clone(),
            |workspace, active| workspace.imp().merge.options.set_modern_pdf(active),
            |workspace| workspace.imp().merge.job.clear_last_output(),
            Self::refresh_view_state,
        );
        let normalize_page_size = output_option_callback(
            self.clone(),
            |workspace, active| {
                workspace
                    .imp()
                    .merge
                    .options
                    .set_normalize_page_size(active)
            },
            |workspace| workspace.imp().merge.job.clear_last_output(),
            Self::refresh_view_state,
        );
        let remove_metadata = output_option_callback(
            self.clone(),
            |workspace, active| workspace.imp().merge.options.set_remove_metadata(active),
            |workspace| workspace.imp().merge.job.clear_last_output(),
            Self::refresh_view_state,
        );
        setup_advanced_options_menu(
            &imp.advanced_options_button,
            imp.merge.options.save_state(),
            AdvancedOptionsMenu::new(modern_pdf, remove_metadata)
                .with_rotate(gettext("Rotate All Files"), rotate_all)
                .with_normalize_page_size(
                    imp.merge.options.normalize_page_size(),
                    normalize_page_size,
                ),
        );

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
            workspace.clear_files();
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
                workspace.add_files(paths, parent);
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

    fn add_files(&self, paths: Vec<PathBuf>, parent: gtk::Window) {
        if paths.is_empty() {
            return;
        }

        self.dismiss_pending_undo();
        let imp = self.imp();
        let paths_to_preview = imp.merge.paths_needing_previews(&paths);
        imp.merge.job.clear_last_output();

        if paths_to_preview.is_empty() {
            imp.merge.add_files(paths);
            self.rebuild_collection(true);
            return;
        }

        imp.merge.job.begin_loading();
        self.refresh_view_state();
        self.load_merge_previews(paths, paths_to_preview, parent);
    }

    fn load_merge_previews(
        &self,
        paths: Vec<PathBuf>,
        paths_to_preview: Vec<PathBuf>,
        parent: gtk::Window,
    ) {
        let workspace = self.clone();
        glib::spawn_future_local(async move {
            let mut loaded_paths = Vec::new();
            let mut loaded_previews = Vec::new();
            let mut loaded_passwords = Vec::new();

            for path in &paths_to_preview {
                match Self::load_merge_preview(&parent, path).await {
                    PdfLoadResult::Loaded {
                        output: preview,
                        password,
                    } => {
                        loaded_paths.push(path.clone());
                        loaded_passwords.push((path.clone(), password));
                        if let Some(preview) = preview {
                            loaded_previews.push((path.clone(), preview));
                        }
                    }
                    PdfLoadResult::Failed(error) => {
                        show_pdf_load_error(&workspace, &error);
                    }
                    PdfLoadResult::Cancelled => {}
                }
            }

            let files_to_add = paths
                .into_iter()
                .filter(|path| !paths_to_preview.contains(path) || loaded_paths.contains(path))
                .collect::<Vec<_>>();
            workspace
                .imp()
                .merge
                .finish_loading(files_to_add, loaded_previews, loaded_passwords);
            workspace.rebuild_collection(true);
        });
    }

    async fn load_merge_preview(
        parent: &gtk::Window,
        path: &Path,
    ) -> PdfLoadResult<Option<crate::preview::PagePreview>> {
        load_processable_pdf(parent, path, |password| {
            crate::preview::render_first_page_preview(path.to_path_buf(), password)
        })
        .await
    }

    fn merge_to(&self, output_file: PathBuf) {
        let imp = self.imp();
        let input_files = imp.merge.pdf_inputs();
        let options = imp.merge.options.options();

        run_output_job(
            self.clone(),
            crate::pdf::merge_pdfs(input_files, output_file, options),
            gettext("Merged PDF saved"),
            |workspace, running| workspace.imp().is_running.set(running),
            |workspace| workspace.imp().merge.job.clear_last_output(),
            |workspace, path| workspace.imp().merge.job.set_last_output(path),
            Self::refresh_view_state,
        );
    }

    pub(super) fn has_view_mode_content(&self) -> bool {
        !self.imp().merge.files.borrow().is_empty()
    }

    pub(super) fn set_view_mode(&self, view_mode: super::ViewMode) {
        self.imp()
            .merge_view_stack
            .set_visible_child_name(view_mode.name());
    }

    fn rebuild_collection(&self, preserve_scroll: bool) {
        let imp = self.imp();
        let scroll_position = if preserve_scroll {
            collection_scroll_position(
                &imp.merge_list_scrolled_window,
                &imp.merge_grid_scrolled_window,
            )
        } else {
            CollectionScrollPosition::default()
        };
        let files = imp.merge.files.borrow();
        let previews = imp.merge.previews.borrow();

        imp.file_list.remove_all();
        imp.merge_file_grid.remove_all();
        for (index, item) in files.iter().enumerate() {
            imp.file_list.append(&self.file_row(
                index,
                &item.path,
                files.len(),
                previews.get(&item.path),
                item.rotation,
            ));
            imp.merge_file_grid.append(&self.file_tile(
                index,
                &item.path,
                files.len(),
                previews.get(&item.path),
                item.rotation,
            ));
        }
        drop(previews);
        drop(files);
        restore_collection_scroll_position(
            &imp.merge_list_scrolled_window,
            &imp.merge_grid_scrolled_window,
            scroll_position,
        );
        self.refresh_view_state();
    }

    fn refresh_item(&self, index: usize) {
        self.refresh_items([index]);
    }

    fn refresh_items(&self, indices: impl IntoIterator<Item = usize>) {
        let imp = self.imp();
        preserve_collection_scroll_position(
            &imp.merge_list_scrolled_window,
            &imp.merge_grid_scrolled_window,
            || {
                for index in indices {
                    self.refresh_item_widgets(index);
                }
            },
        );
        self.refresh_view_state();
    }

    fn refresh_all_items(&self) {
        let count = self.imp().merge.files.borrow().len();
        self.refresh_items(0..count);
    }

    fn refresh_item_widgets(&self, index: usize) {
        let imp = self.imp();
        let files = imp.merge.files.borrow();
        let Some(item) = files.get(index) else {
            return;
        };
        let previews = imp.merge.previews.borrow();
        let row = self.file_row(
            index,
            &item.path,
            files.len(),
            previews.get(&item.path),
            item.rotation,
        );
        let tile = self.file_tile(
            index,
            &item.path,
            files.len(),
            previews.get(&item.path),
            item.rotation,
        );
        replace_collection_item(&imp.file_list, &imp.merge_file_grid, index, &row, &tile);
    }

    pub(super) fn refresh_view_state(&self) {
        let imp = self.imp();
        let files = imp.merge.files.borrow();
        let has_files = !files.is_empty();
        let is_busy = imp.merge.job.is_busy(imp.is_running.get());
        let can_merge = files.len() > 1 && !is_busy;

        imp.empty_status.set_visible(!has_files);
        imp.merge_actions.set_visible(has_files);
        imp.merge_view_stack.set_visible(has_files);
        imp.merge_view_stack.set_sensitive(!is_busy);
        imp.add_button.set_visible(has_files);
        imp.advanced_options_button.set_visible(has_files);
        imp.clear_button.set_visible(has_files);
        imp.merge_button.set_visible(has_files);
        imp.open_output_button
            .set_visible(imp.merge.job.has_last_output());

        imp.add_button.set_sensitive(has_files && !is_busy);
        imp.advanced_options_button
            .set_sensitive(has_files && !is_busy);
        imp.empty_add_button.set_sensitive(!is_busy);
        imp.clear_button.set_sensitive(has_files && !is_busy);
        imp.merge_button.set_sensitive(can_merge);
        imp.open_output_button
            .set_sensitive(imp.merge.job.has_last_output() && !is_busy);

        let count_text = if imp.is_running.get() {
            gettext("Merging PDFs...")
        } else if imp.merge.job.is_loading() {
            gettext("Loading PDFs...")
        } else {
            match files.len() {
                0 => gettext("No files selected"),
                count => ngettext("1 PDF selected", "{} PDFs selected", count as u32)
                    .replace("{}", &count.to_string()),
            }
        };
        update_shell_title(self, PdfTool::Merge, &count_text);
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
            .activatable(true)
            .build();
        row.add_prefix(&list_preview_widget(preview, rotation));

        let options = OrderedItemControlOptions {
            can_move_up: index > 0,
            can_move_down: index + 1 < count,
            can_remove: true,
        };
        let actions = self.file_actions(options, index);
        ordered_item_controls(&actions).append_to_row(&row);

        self.add_file_context_menu(&row, &actions, index);

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
    ) -> gtk::FlowBoxChild {
        let tile = preview_tile();
        tile.append(&tile_preview_widget(preview, rotation));
        tile.append(&tile_label(file_title(path)));

        let controls = tile_controls();
        let size = dim_tile_label(file_subtitle(path));
        controls.append(&size);

        let options = OrderedItemControlOptions {
            can_move_up: index > 0,
            can_move_down: index + 1 < count,
            can_remove: true,
        };
        let actions = self.file_actions(options, index);
        ordered_item_controls(&actions).append_to_box(&controls);

        tile.append(&controls);
        let item = flow_box_item(&tile);
        self.add_file_context_menu(&item, &actions, index);
        self.add_file_drag_and_drop(&item, index);

        item
    }

    fn file_actions(&self, options: OrderedItemControlOptions, index: usize) -> OrderedItemActions {
        let workspace = self.clone();
        let move_up = move || workspace.move_file(index, index.saturating_sub(1));
        let workspace = self.clone();
        let move_down = move || workspace.move_file(index, index.saturating_add(1));
        let workspace = self.clone();
        let rotate = move || workspace.rotate_file(index);
        let workspace = self.clone();
        let remove = move || workspace.remove_file(index);

        OrderedItemActions::new(options, move_up, move_down, rotate, remove)
    }

    fn add_file_context_menu(
        &self,
        widget: &impl IsA<gtk::Widget>,
        actions: &OrderedItemActions,
        index: usize,
    ) {
        let workspace = self.clone();
        let duplicate = move || workspace.duplicate_file(index);
        let mut items = ordered_item_context_menu_items(actions);
        items.insert(
            2,
            ContextMenuItem::new("duplicate", gettext("Du_plicate"), true, duplicate),
        );
        add_item_context_menu(widget, items);
    }

    fn move_file(&self, from: usize, to: usize) {
        if self.is_busy() {
            return;
        }

        if self.imp().merge.move_file(from, to) {
            self.dismiss_pending_undo();
            self.refresh_items(from.min(to)..=from.max(to));
        }
    }

    fn rotate_file(&self, index: usize) {
        if self.is_busy() {
            return;
        }

        if self.imp().merge.rotate_file(index) {
            self.dismiss_pending_undo();
            self.refresh_item(index);
        }
    }

    fn rotate_all_files(&self) {
        if self.is_busy() {
            return;
        }

        if self.imp().merge.rotate_all_files() {
            self.dismiss_pending_undo();
            self.refresh_all_items();
        }
    }

    fn reorder_file(&self, from: usize, to: usize) {
        if self.is_busy() {
            return;
        }

        if self.imp().merge.reorder_file(from, to) {
            self.dismiss_pending_undo();
            self.refresh_items(from.min(to)..=from.max(to));
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
        if self.is_busy() {
            return;
        }

        let undo = self.imp().merge.remove_file(index);
        self.rebuild_collection(true);
        let workspace = self.downgrade();
        self.imp()
            .pending_undo
            .show(self, &gettext("PDF removed"), move || {
                let Some(workspace) = workspace.upgrade() else {
                    return;
                };
                workspace.dismiss_pending_undo();
                workspace.imp().merge.restore_removed_file(undo);
                workspace.rebuild_collection(true);
            });
    }

    fn duplicate_file(&self, index: usize) {
        if self.is_busy() {
            return;
        }

        if self.imp().merge.duplicate_file(index) {
            self.dismiss_pending_undo();
            self.rebuild_collection(true);
        }
    }

    fn open_last_output(&self) {
        if let Some(path) = self.imp().merge.job.last_output() {
            open_output(self, &path);
        }
    }

    fn is_busy(&self) -> bool {
        let imp = self.imp();
        imp.merge.job.is_busy(imp.is_running.get())
    }

    fn clear_files(&self) {
        if self.is_busy() {
            return;
        }

        self.dismiss_pending_undo();
        self.imp().merge.clear();
        self.rebuild_collection(false);
    }

    fn dismiss_pending_undo(&self) {
        self.imp().pending_undo.dismiss();
    }
}
