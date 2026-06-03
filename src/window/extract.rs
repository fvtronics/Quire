use super::ui::{
    format_page_ranges, icon_button, list_preview_widget, open_pdf_file, page_count_label,
    page_ranges_error_message, pdf_file_row, preview_tile, save_pdf_file,
    set_entry_validation_error, tile_controls, tile_label, tile_preview_widget,
};
use super::workspace::{
    add_item_context_menu, collection_scroll_position, flow_box_item, load_single_processable_pdf,
    open_output, output_option_callback, parent_window, preserve_collection_scroll_position,
    replace_collection_item, restore_collection_scroll_position, run_output_job,
    setup_advanced_options_menu, setup_compact_workspace_margins, setup_default_width_breakpoint,
    setup_vertical_layout_breakpoint, show_backend_error, show_toast, update_shell_title,
    update_shell_view_mode, AdvancedOptionsMenu, CollectionScrollPosition, ContextMenuItem,
    SinglePdfLoadHandlers,
};
use super::PdfTool;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use std::path::PathBuf;

mod imp {
    use super::super::state::ExtractState;
    use adw::subclass::prelude::*;
    use gtk::{glib, TemplateChild};
    use std::cell::Cell;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/fvtronics/Quire/extract-workspace.ui")]
    pub struct ExtractWorkspace {
        #[template_child]
        pub extract_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub extract_empty_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub extract_actions: TemplateChild<adw::WrapBox>,
        #[template_child]
        pub extract_empty_status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub extract_content: TemplateChild<gtk::Box>,
        #[template_child]
        pub extract_selection_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub extract_file_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub extract_ranges_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub extract_ranges_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub extract_view_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub extract_page_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub extract_page_grid: TemplateChild<gtk::FlowBox>,
        #[template_child]
        pub extract_list_scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub extract_grid_scrolled_window: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub extract_save_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub extract_advanced_options_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub extract_open_output_button: TemplateChild<gtk::Button>,

        pub extract: ExtractState,
        pub is_running: Cell<bool>,
        pub is_syncing_ranges_entry: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ExtractWorkspace {
        const NAME: &'static str = "ExtractWorkspace";
        type Type = super::ExtractWorkspace;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ExtractWorkspace {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_callbacks();
            obj.refresh_view_state();
        }
    }
    impl WidgetImpl for ExtractWorkspace {}
    impl BoxImpl for ExtractWorkspace {}
}

glib::wrapper! {
    pub struct ExtractWorkspace(ObjectSubclass<imp::ExtractWorkspace>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl ExtractWorkspace {
    fn setup_callbacks(&self) {
        let imp = self.imp();

        let workspace = self.clone();
        let rotate_all = move || workspace.rotate_selected_pages();
        let modern_pdf = output_option_callback(
            self.clone(),
            |workspace, active| workspace.imp().extract.options.set_modern_pdf(active),
            |workspace| workspace.imp().extract.job.clear_last_output(),
            Self::refresh_view_state,
        );
        let normalize_page_size = output_option_callback(
            self.clone(),
            |workspace, active| {
                workspace
                    .imp()
                    .extract
                    .options
                    .set_normalize_page_size(active);
            },
            |workspace| workspace.imp().extract.job.clear_last_output(),
            Self::refresh_view_state,
        );
        let remove_metadata = output_option_callback(
            self.clone(),
            |workspace, active| workspace.imp().extract.options.set_remove_metadata(active),
            |workspace| workspace.imp().extract.job.clear_last_output(),
            Self::refresh_view_state,
        );
        setup_advanced_options_menu(
            &imp.extract_advanced_options_button,
            imp.extract.options.save_state(),
            AdvancedOptionsMenu::new(modern_pdf, remove_metadata)
                .with_rotate(gettext("Rotate Selected Pages"), rotate_all)
                .with_normalize_page_size(
                    imp.extract.options.normalize_page_size(),
                    normalize_page_size,
                ),
        );

        let workspace = self.clone();
        imp.extract_choose_button.connect_clicked(move |_| {
            workspace.choose_file();
        });

        let workspace = self.clone();
        imp.extract_empty_choose_button.connect_clicked(move |_| {
            workspace.choose_file();
        });

        let workspace = self.clone();
        imp.extract_save_button.connect_clicked(move |_| {
            workspace.choose_output_file();
        });

        let workspace = self.clone();
        imp.extract_open_output_button.connect_clicked(move |_| {
            workspace.open_last_output();
        });

        let workspace = self.clone();
        imp.extract_ranges_entry.connect_changed(move |entry| {
            let imp = workspace.imp();
            if imp.is_syncing_ranges_entry.get() {
                return;
            }
            let text = entry.text();
            let text = text.trim();

            let changed_pages = if text.is_empty() {
                imp.extract.clear_range_selection()
            } else if let Ok(pages) =
                crate::pdf::parse_page_ranges(text, imp.extract.page_count.get())
            {
                imp.extract.apply_range_selection(pages)
            } else {
                Vec::new()
            };

            imp.extract.job.clear_last_output();
            if changed_pages.is_empty() {
                workspace.refresh_view_state();
            } else {
                workspace.refresh_items(&changed_pages);
            }
        });
    }

    pub(super) fn setup_responsive_layout(&self, breakpoint: &adw::Breakpoint) {
        let imp = self.imp();
        setup_compact_workspace_margins(breakpoint, self);
        setup_vertical_layout_breakpoint(breakpoint, &imp.extract_selection_box);
        setup_default_width_breakpoint(breakpoint, &*imp.extract_ranges_list);
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
        let imp = self.imp();
        let page_numbers = if imp.extract_ranges_entry.text().trim().is_empty() {
            let pages = imp
                .extract
                .selected_pages
                .borrow()
                .iter()
                .copied()
                .collect::<Vec<_>>();
            if pages.is_empty() {
                show_toast(self, &gettext("Choose at least one page"));
                return;
            }
            pages
        } else {
            match self.extract_pages_from_ranges() {
                Ok(pages) => pages,
                Err(error) => {
                    show_backend_error(self, &error);
                    return;
                }
            }
        };
        let Some((input_file, password, pages)) = imp.extract.selections_from_pages(page_numbers)
        else {
            return;
        };
        let Some(parent) = parent_window(self) else {
            return;
        };

        let workspace = self.clone();
        glib::spawn_future_local(async move {
            if let Some(path) = save_pdf_file(
                &parent,
                &gettext("Save Extracted Pages"),
                &gettext("Extract"),
                "Extracted.pdf",
            )
            .await
            {
                workspace.extract_to(input_file, password, pages, path);
            }
        });
    }

    fn load_pdf(&self, path: PathBuf, parent: gtk::Window) {
        load_single_processable_pdf(
            self.clone(),
            parent,
            path,
            crate::preview::render_page_previews,
            SinglePdfLoadHandlers {
                begin_loading: |workspace: &Self| workspace.imp().extract.job.begin_loading(),
                store_loaded: |workspace: &Self, path, password, previews| {
                    let imp = workspace.imp();
                    imp.extract.load_document(path, password, previews);
                    workspace.set_extract_ranges_entry("");
                    workspace.rebuild_collection(false);
                },
                finish_loading_failed: |workspace: &Self| {
                    workspace.imp().extract.job.finish_loading_failed();
                },
                refresh: Self::refresh_view_state,
            },
        );
    }

    fn extract_to(
        &self,
        input_file: PathBuf,
        password: Option<String>,
        pages: Vec<crate::pdf::PageSelection>,
        output_file: PathBuf,
    ) {
        let options = self.imp().extract.options.options();

        run_output_job(
            self.clone(),
            crate::pdf::extract_pages(input_file, password, pages, output_file, options),
            gettext("Extracted pages saved"),
            |workspace, running| workspace.imp().is_running.set(running),
            |workspace| workspace.imp().extract.job.clear_last_output(),
            |workspace, path| workspace.imp().extract.job.set_last_output(path),
            Self::refresh_view_state,
        );
    }

    pub(super) fn has_view_mode_content(&self) -> bool {
        self.imp().extract.file.borrow().is_some()
    }

    pub(super) fn set_view_mode(&self, view_mode: super::ViewMode) {
        self.imp()
            .extract_view_stack
            .set_visible_child_name(view_mode.name());
    }

    fn rebuild_collection(&self, preserve_scroll: bool) {
        let imp = self.imp();
        let scroll_position = if preserve_scroll {
            collection_scroll_position(
                &imp.extract_list_scrolled_window,
                &imp.extract_grid_scrolled_window,
            )
        } else {
            CollectionScrollPosition::default()
        };
        imp.extract_page_list.remove_all();
        imp.extract_page_grid.remove_all();
        for page_number in 1..=imp.extract.page_count.get() as u32 {
            let (row, tile) = self.extract_page_widgets(page_number);
            imp.extract_page_list.append(&row);
            imp.extract_page_grid.append(&tile);
        }
        restore_collection_scroll_position(
            &imp.extract_list_scrolled_window,
            &imp.extract_grid_scrolled_window,
            scroll_position,
        );
        self.refresh_view_state();
    }

    fn refresh_item(&self, page_number: u32) {
        self.refresh_items(&[page_number]);
    }

    fn refresh_items(&self, page_numbers: &[u32]) {
        let imp = self.imp();
        preserve_collection_scroll_position(
            &imp.extract_list_scrolled_window,
            &imp.extract_grid_scrolled_window,
            || {
                for page_number in page_numbers {
                    let (row, tile) = self.extract_page_widgets(*page_number);
                    replace_collection_item(
                        &self.imp().extract_page_list,
                        &self.imp().extract_page_grid,
                        page_number.saturating_sub(1) as usize,
                        &row,
                        &tile,
                    );
                }
            },
        );
        self.refresh_view_state();
    }

    fn refresh_all_items(&self) {
        let pages = (1..=self.imp().extract.page_count.get() as u32).collect::<Vec<_>>();
        self.refresh_items(&pages);
    }

    fn extract_page_widgets(&self, page_number: u32) -> (adw::ActionRow, gtk::FlowBoxChild) {
        let imp = self.imp();
        let selected_pages = imp.extract.selected_pages.borrow();
        let rotations = imp.extract.rotations.borrow();
        let previews = imp.extract.previews.borrow();
        let preview = previews.get(&page_number);
        let selected = selected_pages.contains(&page_number);
        let rotation = *rotations.get(&page_number).unwrap_or(&0);
        (
            self.extract_page_row(page_number, selected, preview, rotation),
            self.extract_page_tile(page_number, preview, selected, rotation),
        )
    }

    pub(super) fn refresh_view_state(&self) {
        let imp = self.imp();
        let has_file = imp.extract.file.borrow().is_some();
        let has_ranges = !imp.extract_ranges_entry.text().trim().is_empty();
        let has_range_error = has_file && has_ranges && self.extract_pages_from_ranges().is_err();
        let has_valid_ranges = has_ranges && !has_range_error;
        let has_selected_pages = !imp.extract.selected_pages.borrow().is_empty();
        let is_busy = imp.extract.job.is_busy(imp.is_running.get());

        imp.extract_file_list.remove_all();
        if let Some(path) = imp.extract.file.borrow().as_ref() {
            imp.extract_file_list
                .append(&self.extract_file_row(path, imp.extract.page_count.get()));
        }

        imp.extract_empty_status.set_visible(!has_file);
        imp.extract_actions.set_visible(has_file);
        imp.extract_content.set_visible(has_file);
        imp.extract_view_stack.set_sensitive(!is_busy);
        imp.extract_choose_button.set_visible(has_file);
        imp.extract_advanced_options_button.set_visible(has_file);
        imp.extract_save_button.set_visible(has_file);
        imp.extract_open_output_button
            .set_visible(imp.extract.job.has_last_output());

        imp.extract_choose_button.set_sensitive(!is_busy);
        imp.extract_advanced_options_button
            .set_sensitive(has_file && !is_busy);
        imp.extract_empty_choose_button.set_sensitive(!is_busy);
        imp.extract_save_button.set_sensitive(
            has_file && (has_valid_ranges || (!has_ranges && has_selected_pages)) && !is_busy,
        );
        imp.extract_open_output_button
            .set_sensitive(imp.extract.job.has_last_output() && !is_busy);
        imp.extract_ranges_entry.set_sensitive(has_file && !is_busy);
        set_entry_validation_error(
            &imp.extract_ranges_entry,
            has_range_error,
            &page_ranges_error_message(),
        );

        let detail = if imp.is_running.get() {
            gettext("Extracting pages...")
        } else if imp.extract.job.is_loading() {
            gettext("Loading PDF...")
        } else if has_file {
            page_count_label(imp.extract.page_count.get())
        } else {
            gettext("No PDF selected")
        };
        update_shell_title(self, PdfTool::Extract, &detail);
        update_shell_view_mode(self);
    }

    fn extract_pages_from_ranges(&self) -> Result<Vec<u32>, crate::pdf::PdfBackendError> {
        let imp = self.imp();
        crate::pdf::parse_page_ranges(
            imp.extract_ranges_entry.text().as_str(),
            imp.extract.page_count.get(),
        )
    }

    fn extract_file_row(&self, path: &std::path::Path, page_count: usize) -> adw::ActionRow {
        pdf_file_row(path, page_count_label(page_count))
    }

    fn extract_page_row(
        &self,
        page_number: u32,
        selected: bool,
        preview: Option<&crate::preview::PagePreview>,
        rotation: i64,
    ) -> adw::ActionRow {
        let check_button = gtk::CheckButton::builder()
            .active(selected)
            .tooltip_text(gettext("Select Page"))
            .valign(gtk::Align::Center)
            .build();
        let row = adw::ActionRow::builder()
            .title(format!("{} {page_number}", gettext("Page")))
            .title_lines(1)
            .activatable(true)
            .activatable_widget(&check_button)
            .build();

        row.add_prefix(&list_preview_widget(preview, rotation));

        let rotate_button =
            icon_button("object-rotate-right-symbolic", &gettext("Rotate Clockwise"));
        rotate_button.set_sensitive(selected);
        let window = self.clone();
        rotate_button.connect_clicked(move |_| {
            window.rotate_extract_page(page_number);
        });
        row.add_suffix(&rotate_button);

        row.add_suffix(&check_button);

        let window = self.clone();
        check_button.connect_toggled(move |button| {
            window.toggle_extract_page(page_number, button.is_active());
        });

        self.add_page_context_menu(&row, page_number, selected);

        row
    }

    fn extract_page_tile(
        &self,
        page_number: u32,
        preview: Option<&crate::preview::PagePreview>,
        selected: bool,
        rotation: i64,
    ) -> gtk::FlowBoxChild {
        let tile = preview_tile();
        let preview_widget = tile_preview_widget(preview, rotation);
        tile.append(&preview_widget);

        let footer = tile_controls();
        let label = tile_label(format!("{} {}", gettext("Page"), page_number));
        label.set_hexpand(true);
        let rotate_button =
            icon_button("object-rotate-right-symbolic", &gettext("Rotate Clockwise"));
        rotate_button.set_sensitive(selected);
        let window = self.clone();
        rotate_button.connect_clicked(move |_| {
            window.rotate_extract_page(page_number);
        });
        let check_button = gtk::CheckButton::builder()
            .active(selected)
            .tooltip_text(gettext("Select Page"))
            .valign(gtk::Align::Center)
            .build();

        footer.append(&label);
        footer.append(&rotate_button);
        footer.append(&check_button);
        tile.append(&footer);

        let window = self.clone();
        check_button.connect_toggled(move |button| {
            window.toggle_extract_page(page_number, button.is_active());
        });

        let item = flow_box_item(&tile);
        let window = self.clone();
        item.connect_activate(move |_| {
            window.toggle_extract_page(page_number, !selected);
        });
        self.add_page_context_menu(&item, page_number, selected);

        item
    }

    fn add_page_context_menu(
        &self,
        widget: &impl IsA<gtk::Widget>,
        page_number: u32,
        selected: bool,
    ) {
        let workspace = self.clone();
        let rotate = move || workspace.rotate_extract_page(page_number);
        let workspace = self.clone();
        let toggle = move || workspace.toggle_extract_page(page_number, !selected);

        add_item_context_menu(
            widget,
            vec![
                ContextMenuItem::new("rotate", gettext("Rotate _Clockwise"), selected, rotate),
                ContextMenuItem::new(
                    "toggle",
                    if selected {
                        gettext("_Unselect")
                    } else {
                        gettext("_Select")
                    },
                    true,
                    toggle,
                ),
            ],
        );
    }

    fn toggle_extract_page(&self, page_number: u32, selected: bool) {
        if self.is_busy() {
            return;
        }

        if self.imp().extract.toggle_page(page_number, selected) {
            self.update_extract_ranges_entry();
            self.refresh_item(page_number);
        }
    }

    fn rotate_extract_page(&self, page_number: u32) {
        if self.is_busy() {
            return;
        }

        if self.imp().extract.rotate_page(page_number) {
            self.refresh_item(page_number);
        }
    }

    fn rotate_selected_pages(&self) {
        if self.is_busy() {
            return;
        }

        if self.imp().extract.rotate_selected_pages() {
            self.refresh_all_items();
        }
    }

    fn is_busy(&self) -> bool {
        let imp = self.imp();
        imp.extract.job.is_busy(imp.is_running.get())
    }

    fn update_extract_ranges_entry(&self) {
        let imp = self.imp();
        let text = {
            let pages = imp.extract.selected_pages.borrow();
            let pages = pages.iter().copied().collect::<Vec<_>>();
            format_page_ranges(&pages)
        };

        self.set_extract_ranges_entry(&text);
    }

    fn set_extract_ranges_entry(&self, text: &str) {
        let imp = self.imp();
        if imp.extract_ranges_entry.text().as_str() == text {
            return;
        }

        imp.is_syncing_ranges_entry.set(true);
        imp.extract_ranges_entry.set_text(text);
        imp.is_syncing_ranges_entry.set(false);
    }

    fn open_last_output(&self) {
        if let Some(path) = self.imp().extract.job.last_output() {
            open_output(self, &path);
        }
    }
}
