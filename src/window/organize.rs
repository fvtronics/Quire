use super::ui::{
    blank_list_preview_widget, blank_tile_preview_widget, dim_tile_label, list_preview_widget,
    open_pdf_file, page_count_label, preview_tile, save_pdf_file, tile_controls, tile_label,
    tile_preview_widget,
};
use super::workspace::{
    add_item_context_menu, collection_scroll_position, load_single_processable_pdf, open_output,
    ordered_item_context_menu_items, ordered_item_controls, output_option_callback, parent_window,
    preserve_collection_scroll_position, replace_collection_item,
    restore_collection_scroll_position, run_output_job, setup_advanced_options_menu,
    update_shell_title, update_shell_view_mode, AdvancedOptionsMenu, CollectionScrollPosition,
    ContextMenuItem, OrderedItemActions, OrderedItemControlOptions, PendingUndo,
    SinglePdfLoadHandlers,
};
use super::PdfTool;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use std::path::PathBuf;

mod imp {
    use super::super::state::OrganizeState;
    use super::PendingUndo;
    use adw::subclass::prelude::*;
    use gtk::{glib, TemplateChild};
    use std::cell::Cell;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/fvtronics/folios/organize-workspace.ui")]
    pub struct OrganizeWorkspace {
        #[template_child]
        pub organize_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub organize_empty_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub organize_actions: TemplateChild<gtk::Box>,
        #[template_child]
        pub organize_empty_status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub organize_view_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub organize_page_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub organize_page_grid: TemplateChild<gtk::FlowBox>,
        #[template_child]
        pub organize_list_scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub organize_grid_scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub organize_reset_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub organize_advanced_options_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub organize_save_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub organize_open_output_button: TemplateChild<gtk::Button>,

        pub organize: OrganizeState,
        pub(super) pending_undo: PendingUndo,
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
            obj.refresh_view_state();
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
        let rotate_all = move || workspace.rotate_all_pages();
        let modern_pdf = output_option_callback(
            self.clone(),
            |workspace, active| workspace.imp().organize.options.set_modern_pdf(active),
            |workspace| workspace.imp().organize.job.clear_last_output(),
            Self::refresh_view_state,
        );
        let normalize_page_size = output_option_callback(
            self.clone(),
            |workspace, active| {
                workspace
                    .imp()
                    .organize
                    .options
                    .set_normalize_page_size(active);
            },
            |workspace| workspace.imp().organize.job.clear_last_output(),
            Self::refresh_view_state,
        );
        let remove_metadata = output_option_callback(
            self.clone(),
            |workspace, active| workspace.imp().organize.options.set_remove_metadata(active),
            |workspace| workspace.imp().organize.job.clear_last_output(),
            Self::refresh_view_state,
        );
        setup_advanced_options_menu(
            &imp.organize_advanced_options_button,
            imp.organize.options.save_state(),
            AdvancedOptionsMenu::new(modern_pdf, remove_metadata)
                .with_rotate(gettext("Rotate All Pages"), rotate_all)
                .with_normalize_page_size(
                    imp.organize.options.normalize_page_size(),
                    normalize_page_size,
                ),
        );

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
                workspace.load_pdf(path, parent);
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

    fn load_pdf(&self, path: PathBuf, parent: gtk::Window) {
        self.dismiss_pending_undo();
        load_single_processable_pdf(
            self.clone(),
            parent,
            path,
            crate::preview::render_page_previews,
            SinglePdfLoadHandlers {
                begin_loading: |workspace: &Self| workspace.imp().organize.job.begin_loading(),
                store_loaded: |workspace: &Self, path, password, previews| {
                    workspace
                        .imp()
                        .organize
                        .load_document(path, password, previews);
                    workspace.rebuild_collection(false);
                },
                finish_loading_failed: |workspace: &Self| {
                    workspace.imp().organize.job.finish_loading_failed();
                },
                refresh: Self::refresh_view_state,
            },
        );
    }

    fn reset_pdf(&self) {
        if let Some(undo) = self.imp().organize.reset() {
            self.rebuild_collection(true);
            let workspace = self.downgrade();
            self.imp()
                .pending_undo
                .show(self, &gettext("Page order reset"), move || {
                    let Some(workspace) = workspace.upgrade() else {
                        return;
                    };
                    workspace.dismiss_pending_undo();
                    workspace.imp().organize.restore_reset(undo);
                    workspace.rebuild_collection(true);
                });
        }
    }

    fn organize_to(&self, output_file: PathBuf) {
        let imp = self.imp();
        let Some((input_file, password, page_order)) = imp.organize.selections() else {
            return;
        };
        let options = imp.organize.options.options();

        run_output_job(
            self.clone(),
            crate::pdf::organize_pdf(input_file, password, page_order, output_file, options),
            gettext("Organized PDF saved"),
            |workspace, running| workspace.imp().is_running.set(running),
            |workspace| workspace.imp().organize.job.clear_last_output(),
            |workspace, path| workspace.imp().organize.job.set_last_output(path),
            Self::refresh_view_state,
        );
    }

    pub(super) fn has_view_mode_content(&self) -> bool {
        self.imp().organize.file.borrow().is_some()
    }

    pub(super) fn set_view_mode(&self, view_mode: super::ViewMode) {
        self.imp()
            .organize_view_stack
            .set_visible_child_name(view_mode.name());
    }

    fn rebuild_collection(&self, preserve_scroll: bool) {
        let imp = self.imp();
        let scroll_position = if preserve_scroll {
            collection_scroll_position(
                &imp.organize_list_scrolled_window,
                &imp.organize_grid_scrolled_window,
            )
        } else {
            CollectionScrollPosition::default()
        };
        let page_order = imp.organize.page_order.borrow();
        let previews = imp.organize.previews.borrow();

        imp.organize_page_list.remove_all();
        imp.organize_page_grid.remove_all();
        for (index, page) in page_order.iter().enumerate() {
            let preview = previews.get(&page.page_number);
            imp.organize_page_list
                .append(&self.page_row(index, *page, page_order.len(), preview));
            imp.organize_page_grid.append(&self.organize_page_tile(
                *page,
                preview,
                index,
                page_order.len(),
            ));
        }
        drop(previews);
        drop(page_order);
        restore_collection_scroll_position(
            &imp.organize_list_scrolled_window,
            &imp.organize_grid_scrolled_window,
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
            &imp.organize_list_scrolled_window,
            &imp.organize_grid_scrolled_window,
            || {
                for index in indices {
                    self.refresh_item_widgets(index);
                }
            },
        );
        self.refresh_view_state();
    }

    fn refresh_all_items(&self) {
        let count = self.imp().organize.page_order.borrow().len();
        self.refresh_items(0..count);
    }

    fn refresh_item_widgets(&self, index: usize) {
        let imp = self.imp();
        let page_order = imp.organize.page_order.borrow();
        let Some(page) = page_order.get(index).copied() else {
            return;
        };
        let previews = imp.organize.previews.borrow();
        let preview = previews.get(&page.page_number);
        let row = self.page_row(index, page, page_order.len(), preview);
        let tile = self.organize_page_tile(page, preview, index, page_order.len());
        replace_collection_item(
            &imp.organize_page_list,
            &imp.organize_page_grid,
            index,
            &row,
            &tile,
        );
    }

    pub(super) fn refresh_view_state(&self) {
        let imp = self.imp();
        let page_order = imp.organize.page_order.borrow();
        let has_file = imp.organize.file.borrow().is_some();
        let has_pages = !page_order.is_empty();
        let is_busy = imp.organize.job.is_busy(imp.is_running.get());

        imp.organize_empty_status.set_visible(!has_file);
        imp.organize_actions.set_visible(has_file);
        imp.organize_view_stack.set_visible(has_file);
        imp.organize_view_stack.set_sensitive(!is_busy);
        imp.organize_choose_button.set_visible(has_file);
        imp.organize_advanced_options_button.set_visible(has_file);
        imp.organize_reset_button.set_visible(has_file);
        imp.organize_save_button.set_visible(has_file);
        imp.organize_open_output_button
            .set_visible(imp.organize.job.has_last_output());

        imp.organize_choose_button.set_sensitive(!is_busy);
        imp.organize_advanced_options_button
            .set_sensitive(has_file && !is_busy);
        imp.organize_empty_choose_button.set_sensitive(!is_busy);
        imp.organize_reset_button
            .set_sensitive(has_file && !is_busy);
        imp.organize_save_button
            .set_sensitive(has_pages && !is_busy);
        imp.organize_open_output_button
            .set_sensitive(imp.organize.job.has_last_output() && !is_busy);

        let detail = if imp.is_running.get() {
            gettext("Organizing pages...")
        } else if imp.organize.job.is_loading() {
            gettext("Loading PDF...")
        } else if has_file {
            page_count_label(page_order.len())
        } else {
            gettext("No PDF selected")
        };
        update_shell_title(self, PdfTool::Organize, &detail);
        update_shell_view_mode(self);
    }

    fn page_row(
        &self,
        index: usize,
        page: crate::pdf::PageSelection,
        count: usize,
        preview: Option<&crate::preview::PagePreview>,
    ) -> adw::ActionRow {
        let row = adw::ActionRow::builder()
            .title(page_title(page))
            .subtitle(format!("{} {}/{}", gettext("Position"), index + 1, count))
            .activatable(true)
            .build();

        if page.is_blank() {
            row.add_prefix(&blank_list_preview_widget(preview, page.rotation));
        } else {
            row.add_prefix(&list_preview_widget(preview, page.rotation));
        }

        let options = OrderedItemControlOptions {
            can_move_up: index > 0,
            can_move_down: index + 1 < count,
            can_remove: count > 1,
        };
        let actions = self.page_actions(options, index);
        ordered_item_controls(&actions).append_to_row(&row);

        self.add_page_context_menu(&row, &actions, index);

        self.add_page_drag_and_drop(&row, index);

        row
    }

    fn organize_page_tile(
        &self,
        page: crate::pdf::PageSelection,
        preview: Option<&crate::preview::PagePreview>,
        index: usize,
        count: usize,
    ) -> gtk::Box {
        let tile = preview_tile();
        if page.is_blank() {
            tile.append(&blank_tile_preview_widget(preview, page.rotation));
        } else {
            tile.append(&tile_preview_widget(preview, page.rotation));
        }
        tile.append(&tile_label(page_title(page)));

        let controls = tile_controls();
        let position = dim_tile_label(format!("{}/{}", index + 1, count));
        controls.append(&position);

        let options = OrderedItemControlOptions {
            can_move_up: index > 0,
            can_move_down: index + 1 < count,
            can_remove: count > 1,
        };
        let actions = self.page_actions(options, index);
        ordered_item_controls(&actions).append_to_box(&controls);

        tile.append(&controls);
        self.add_page_context_menu(&tile, &actions, index);

        self.add_page_drag_and_drop(&tile, index);

        tile
    }

    fn page_actions(&self, options: OrderedItemControlOptions, index: usize) -> OrderedItemActions {
        let workspace = self.clone();
        let move_up = move || workspace.move_page(index, index.saturating_sub(1));
        let workspace = self.clone();
        let move_down = move || workspace.move_page(index, index.saturating_add(1));
        let workspace = self.clone();
        let rotate = move || workspace.rotate_page(index);
        let workspace = self.clone();
        let remove = move || workspace.remove_page(index);

        OrderedItemActions::new(options, move_up, move_down, rotate, remove)
    }

    fn add_page_context_menu(
        &self,
        widget: &impl IsA<gtk::Widget>,
        actions: &OrderedItemActions,
        index: usize,
    ) {
        let workspace = self.clone();
        let insert_blank = move || workspace.insert_blank_page_after(index);
        let workspace = self.clone();
        let duplicate = move || workspace.duplicate_page(index);

        let mut items = ordered_item_context_menu_items(actions);
        items.splice(
            2..2,
            [
                ContextMenuItem::new(
                    "insert-blank",
                    gettext("Insert Blank Page After"),
                    true,
                    insert_blank,
                ),
                ContextMenuItem::new("duplicate", gettext("Duplicate"), true, duplicate),
            ],
        );
        add_item_context_menu(widget, items);
    }

    fn move_page(&self, from: usize, to: usize) {
        if self.is_busy() {
            return;
        }

        if self.imp().organize.move_page(from, to) {
            self.dismiss_pending_undo();
            self.refresh_items(from.min(to)..=from.max(to));
        }
    }

    fn rotate_page(&self, index: usize) {
        if self.is_busy() {
            return;
        }

        if self.imp().organize.rotate_page(index) {
            self.dismiss_pending_undo();
            self.refresh_item(index);
        }
    }

    fn rotate_all_pages(&self) {
        if self.is_busy() {
            return;
        }

        if self.imp().organize.rotate_all_pages() {
            self.dismiss_pending_undo();
            self.refresh_all_items();
        }
    }

    fn reorder_page(&self, from: usize, to: usize) {
        if self.is_busy() {
            return;
        }

        if self.imp().organize.reorder_page(from, to) {
            self.dismiss_pending_undo();
            self.refresh_items(from.min(to)..=from.max(to));
        }
    }

    fn add_page_drag_and_drop(&self, widget: &impl IsA<gtk::Widget>, index: usize) {
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

            window.reorder_page(from as usize, index);
            true
        });
        widget.add_controller(drop_target);
    }

    fn remove_page(&self, index: usize) {
        if self.is_busy() {
            return;
        }

        if let Some(undo) = self.imp().organize.remove_page(index) {
            self.rebuild_collection(true);
            let workspace = self.downgrade();
            self.imp()
                .pending_undo
                .show(self, &gettext("Page removed"), move || {
                    let Some(workspace) = workspace.upgrade() else {
                        return;
                    };
                    workspace.dismiss_pending_undo();
                    workspace.imp().organize.restore_removed_page(undo);
                    workspace.rebuild_collection(true);
                });
        }
    }

    fn insert_blank_page_after(&self, index: usize) {
        if self.is_busy() {
            return;
        }

        if self.imp().organize.insert_blank_page_after(index) {
            self.dismiss_pending_undo();
            self.rebuild_collection(true);
        }
    }

    fn duplicate_page(&self, index: usize) {
        if self.is_busy() {
            return;
        }

        if self.imp().organize.duplicate_page(index) {
            self.dismiss_pending_undo();
            self.rebuild_collection(true);
        }
    }

    fn open_last_output(&self) {
        if let Some(path) = self.imp().organize.job.last_output() {
            open_output(self, &path);
        }
    }

    fn is_busy(&self) -> bool {
        let imp = self.imp();
        imp.organize.job.is_busy(imp.is_running.get())
    }

    fn dismiss_pending_undo(&self) {
        self.imp().pending_undo.dismiss();
    }
}

fn page_title(page: crate::pdf::PageSelection) -> String {
    if page.is_blank() {
        gettext("Blank Page")
    } else {
        format!("{} {}", gettext("Page"), page.page_number)
    }
}
