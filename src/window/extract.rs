use super::ui::{
    format_page_ranges, icon_button, normalize_pages, open_pdf_file, page_count_label,
    pdf_file_row, preview_tile, rotated_list_preview_prefix, save_pdf_file, tile_controls,
    tile_label, tile_preview_widget,
};
use super::workspace::{
    load_single_processable_pdf, open_output, output_option_callback, parent_window,
    run_output_job, setup_advanced_options_menu, show_backend_error, show_toast,
    update_shell_title, update_shell_view_mode, AdvancedOptionsMenu, SinglePdfLoadHandlers,
};
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
    #[template(resource = "/com/fvtronics/folios/extract-workspace.ui")]
    pub struct ExtractWorkspace {
        #[template_child]
        pub extract_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub extract_empty_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub extract_actions: TemplateChild<gtk::Box>,
        #[template_child]
        pub extract_empty_status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub extract_content: TemplateChild<gtk::Box>,
        #[template_child]
        pub extract_file_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub extract_ranges_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub extract_view_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub extract_page_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub extract_page_grid: TemplateChild<gtk::FlowBox>,
        #[template_child]
        pub extract_save_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub extract_advanced_options_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub extract_open_output_button: TemplateChild<gtk::Button>,

        pub extract: ExtractState,
        pub is_running: Cell<bool>,
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
            obj.update_view();
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
            Self::update_view,
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
            Self::update_view,
        );
        let remove_metadata = output_option_callback(
            self.clone(),
            |workspace, active| workspace.imp().extract.options.set_remove_metadata(active),
            |workspace| workspace.imp().extract.job.clear_last_output(),
            Self::update_view,
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
            let text = entry.text();
            let text = text.trim();

            if text.is_empty() {
                imp.extract.clear_range_selection();
            } else if let Ok(pages) =
                crate::pdf::parse_page_ranges(text, imp.extract.page_count.get())
            {
                let pages = normalize_pages(pages);
                imp.extract.apply_range_selection(pages);
            }

            imp.extract.job.clear_last_output();
            workspace.update_view();
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
                    imp.extract_ranges_entry.set_text("");
                },
                finish_loading_failed: |workspace: &Self| {
                    workspace.imp().extract.job.finish_loading_failed();
                },
                refresh: Self::update_view,
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
            Self::update_view,
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

    pub(super) fn update_view(&self) {
        let imp = self.imp();
        let has_file = imp.extract.file.borrow().is_some();
        let has_ranges = !imp.extract_ranges_entry.text().trim().is_empty();
        let has_valid_ranges = has_ranges && self.extract_pages_from_ranges().is_ok();
        let has_selected_pages = !imp.extract.selected_pages.borrow().is_empty();
        let is_busy = imp.extract.job.is_busy(imp.is_running.get());

        imp.extract_file_list.remove_all();
        if let Some(path) = imp.extract.file.borrow().as_ref() {
            imp.extract_file_list
                .append(&self.extract_file_row(path, imp.extract.page_count.get()));
        }

        imp.extract_page_list.remove_all();
        imp.extract_page_grid.remove_all();
        let selected_pages = imp.extract.selected_pages.borrow();
        let rotations = imp.extract.rotations.borrow();
        let previews = imp.extract.previews.borrow();
        for page_number in 1..=imp.extract.page_count.get() as u32 {
            let preview = previews.get(&page_number);
            let selected = selected_pages.contains(&page_number);
            let rotation = *rotations.get(&page_number).unwrap_or(&0);
            imp.extract_page_list.append(&self.extract_page_row(
                page_number,
                selected,
                preview,
                rotation,
            ));
            imp.extract_page_grid.append(&self.extract_page_tile(
                page_number,
                preview,
                selected,
                rotation,
            ));
        }

        imp.extract_empty_status.set_visible(!has_file);
        imp.extract_actions.set_visible(has_file);
        imp.extract_content.set_visible(has_file);
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

        let detail = if imp.is_running.get() {
            gettext("Extracting pages...")
        } else if imp.extract.job.is_loading() {
            gettext("Loading PDF...")
        } else if has_file {
            page_count_label(imp.extract.page_count.get())
        } else {
            gettext("No PDF selected")
        };
        update_shell_title(self, &gettext("Extract Pages"), &detail);
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
            .sensitive(!self.is_busy())
            .tooltip_text(gettext("Select Page"))
            .valign(gtk::Align::Center)
            .build();
        let row = adw::ActionRow::builder()
            .title(format!("{} {page_number}", gettext("Page")))
            .activatable(true)
            .activatable_widget(&check_button)
            .build();

        row.add_prefix(&rotated_list_preview_prefix(preview, rotation));

        let rotate_button =
            icon_button("object-rotate-right-symbolic", &gettext("Rotate Clockwise"));
        rotate_button.set_sensitive(selected && !self.is_busy());
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

        row
    }

    fn extract_page_tile(
        &self,
        page_number: u32,
        preview: Option<&crate::preview::PagePreview>,
        selected: bool,
        rotation: i64,
    ) -> gtk::Box {
        let tile = preview_tile();
        let preview_widget = tile_preview_widget(preview, rotation);
        let window = self.clone();
        let click = gtk::GestureClick::new();
        click.connect_released(move |_, _, _, _| {
            window.toggle_extract_page(page_number, !selected);
        });
        preview_widget.add_controller(click);
        tile.append(&preview_widget);

        let footer = tile_controls();
        let label = tile_label(format!("{} {}", gettext("Page"), page_number));
        label.set_hexpand(true);
        let rotate_button =
            icon_button("object-rotate-right-symbolic", &gettext("Rotate Clockwise"));
        rotate_button.set_sensitive(selected && !self.is_busy());
        let window = self.clone();
        rotate_button.connect_clicked(move |_| {
            window.rotate_extract_page(page_number);
        });
        let check_button = gtk::CheckButton::builder()
            .active(selected)
            .sensitive(!self.is_busy())
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

        tile
    }

    fn toggle_extract_page(&self, page_number: u32, selected: bool) {
        if self.is_busy() {
            return;
        }

        self.imp().extract.toggle_page(page_number, selected);
        self.update_extract_ranges_entry();
        self.update_view();
    }

    fn rotate_extract_page(&self, page_number: u32) {
        if self.is_busy() {
            return;
        }

        if self.imp().extract.rotate_page(page_number) {
            self.update_view();
        }
    }

    fn rotate_selected_pages(&self) {
        if self.is_busy() {
            return;
        }

        if self.imp().extract.rotate_selected_pages() {
            self.update_view();
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

        if imp.extract_ranges_entry.text().as_str() != text {
            imp.extract_ranges_entry.set_text(&text);
        }
    }

    fn open_last_output(&self) {
        if let Some(path) = self.imp().extract.job.last_output() {
            open_output(self, &path);
        }
    }
}
