use super::PdfTool;
use super::ui::{
    DelayedEntryValidationState, EntryValidation, clear_box, connect_delayed_entry_validation,
    file_title, open_pdf_file, output_pdf_name, save_pdf_file, single_file_preview_widget,
};
use super::workspace::{
    AdaptiveBreakpoints, AdvancedOptionsMenu, SinglePdfLoadHandlers, load_single_processable_pdf,
    open_output, output_option_callback, parent_window, run_output_job,
    setup_advanced_options_menu, setup_compact_workspace_margins, setup_default_height_breakpoint,
    setup_default_width_breakpoint, setup_short_narrow_icon_buttons, setup_short_narrow_preview,
    setup_short_status_page, setup_vertical_layout_breakpoint, update_shell_title,
    update_shell_view_mode,
};
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use std::path::PathBuf;

fn keywords_error_message() -> String {
    gettext("Enter keywords separated by commas.")
}

mod imp {
    use super::super::state::MetadataState;
    use super::DelayedEntryValidationState;
    use adw::subclass::prelude::*;
    use gtk::{TemplateChild, glib};
    use std::cell::Cell;
    use std::rc::Rc;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/fvtronics/Quire/metadata-workspace.ui")]
    pub struct MetadataWorkspace {
        #[template_child]
        pub metadata_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub metadata_empty_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub metadata_actions: TemplateChild<adw::WrapBox>,
        #[template_child]
        pub metadata_empty_status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub metadata_content: TemplateChild<gtk::Box>,
        #[template_child]
        pub metadata_preview_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub metadata_title_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub metadata_author_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub metadata_subject_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub metadata_keywords_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub metadata_keywords_error_row: TemplateChild<gtk::ListBoxRow>,
        #[template_child]
        pub metadata_keywords_error_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub metadata_creator_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub metadata_producer_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub metadata_save_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub metadata_open_output_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub metadata_advanced_options_button: TemplateChild<gtk::MenuButton>,

        pub metadata: MetadataState,
        pub is_running: Cell<bool>,
        pub(super) keywords_validation: Rc<DelayedEntryValidationState>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MetadataWorkspace {
        const NAME: &'static str = "MetadataWorkspace";
        type Type = super::MetadataWorkspace;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MetadataWorkspace {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_callbacks();
            obj.update_view();
        }
    }
    impl WidgetImpl for MetadataWorkspace {}
    impl BoxImpl for MetadataWorkspace {}
}

glib::wrapper! {
    pub struct MetadataWorkspace(ObjectSubclass<imp::MetadataWorkspace>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl MetadataWorkspace {
    fn setup_callbacks(&self) {
        let imp = self.imp();

        let modern_pdf = output_option_callback(
            self.clone(),
            |workspace, active| workspace.imp().metadata.options.set_modern_pdf(active),
            |workspace| workspace.imp().metadata.job.clear_last_output(),
            Self::update_view,
        );
        let remove_metadata = output_option_callback(
            self.clone(),
            |workspace, active| workspace.imp().metadata.options.set_remove_metadata(active),
            |workspace| workspace.imp().metadata.job.clear_last_output(),
            Self::update_view,
        );
        setup_advanced_options_menu(
            &imp.metadata_advanced_options_button,
            &imp.metadata.options,
            AdvancedOptionsMenu::new(modern_pdf, remove_metadata),
        );
        imp.metadata_author_entry
            .set_input_purpose(gtk::InputPurpose::Name);

        let producer_warning = gtk::Image::from_icon_name("dialog-warning-symbolic");
        producer_warning.set_tooltip_text(Some(&gettext("Producer will be replaced when saving")));
        imp.metadata_producer_row.add_suffix(&producer_warning);

        let workspace = self.clone();
        imp.metadata_choose_button.connect_clicked(move |_| {
            workspace.choose_file();
        });

        let workspace = self.clone();
        imp.metadata_empty_choose_button.connect_clicked(move |_| {
            workspace.choose_file();
        });

        let workspace = self.clone();
        imp.metadata_save_button.connect_clicked(move |_| {
            workspace.choose_output_file();
        });

        let workspace = self.clone();
        imp.metadata_open_output_button.connect_clicked(move |_| {
            workspace.open_last_output();
        });

        self.connect_metadata_changed(&imp.metadata_title_entry);
        self.connect_metadata_changed(&imp.metadata_author_entry);
        self.connect_metadata_changed(&imp.metadata_subject_entry);

        let workspace = self.clone();
        let keywords_changed = move || {
            workspace.imp().metadata.job.clear_last_output();
            workspace.update_view();
        };

        let workspace = self.clone();
        let refresh_keywords_validation = move || {
            workspace.update_view();
        };
        connect_delayed_entry_validation(
            &imp.metadata_keywords_entry,
            imp.keywords_validation.clone(),
            keywords_changed,
            refresh_keywords_validation,
        );
    }

    pub(super) fn setup_responsive_layout(&self, breakpoints: &AdaptiveBreakpoints) {
        let imp = self.imp();
        setup_compact_workspace_margins(breakpoints, self);
        setup_short_status_page(breakpoints, &imp.metadata_empty_status);
        setup_vertical_layout_breakpoint(breakpoints, &imp.metadata_content);
        setup_default_width_breakpoint(breakpoints, &*imp.metadata_preview_box);
        setup_default_height_breakpoint(breakpoints, &*imp.metadata_preview_box);
        setup_short_narrow_preview(breakpoints, &*imp.metadata_preview_box);
        setup_short_narrow_icon_buttons(
            breakpoints,
            &[
                (&imp.metadata_choose_button, "document-open-symbolic"),
                (&imp.metadata_open_output_button, "arrow-into-box-symbolic"),
                (&imp.metadata_save_button, "document-save-symbolic"),
            ],
        );
    }

    fn connect_metadata_changed(&self, entry: &adw::EntryRow) {
        let workspace = self.clone();
        entry.connect_changed(move |_| {
            workspace.imp().metadata.job.clear_last_output();
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
        let Some((input_file, password)) = self.imp().metadata.input_file() else {
            return;
        };
        let Some(parent) = parent_window(self) else {
            return;
        };
        let Ok(metadata) = self.metadata_from_entries() else {
            return;
        };
        let options = self.imp().metadata.options.options();

        let workspace = self.clone();
        glib::spawn_future_local(async move {
            let initial_name = output_pdf_name(&input_file, "metadata");
            if let Some(path) = save_pdf_file(
                &parent,
                &gettext("Save PDF Metadata"),
                &gettext("Save"),
                &initial_name,
            )
            .await
            {
                workspace.save_metadata_to(input_file, password, path, metadata, options);
            }
        });
    }

    fn load_pdf(&self, path: PathBuf, parent: gtk::Window) {
        load_single_processable_pdf(
            self.clone(),
            parent,
            path,
            crate::preview::render_single_file_preview_with_metadata,
            SinglePdfLoadHandlers {
                begin_loading: |workspace: &Self| workspace.imp().metadata.job.begin_loading(),
                store_loaded: |workspace: &Self, path, password, (preview, metadata)| {
                    workspace
                        .imp()
                        .metadata
                        .finish_loading(path, password, preview);
                    workspace.set_metadata_entries(&metadata);
                },
                finish_loading_failed: |workspace: &Self| {
                    workspace.imp().metadata.job.finish_loading_failed();
                },
                refresh: Self::update_view,
            },
        );
    }

    fn save_metadata_to(
        &self,
        input_file: PathBuf,
        password: Option<String>,
        output_file: PathBuf,
        metadata: crate::pdf::PdfEditableMetadata,
        options: crate::pdf::PdfSaveOptions,
    ) {
        run_output_job(
            self.clone(),
            crate::pdf::edit_pdf_metadata(input_file, password, output_file, metadata, options),
            gettext("PDF metadata saved"),
            |workspace, running| workspace.imp().is_running.set(running),
            |workspace| workspace.imp().metadata.job.clear_last_output(),
            |workspace, path| workspace.imp().metadata.job.set_last_output(path),
            Self::update_view,
        );
    }

    pub(super) fn update_view(&self) {
        let imp = self.imp();
        let file = imp.metadata.file.borrow();
        let has_file = file.is_some();
        let has_keywords_error =
            has_file && normalize_keywords(imp.metadata_keywords_entry.text().as_str()).is_err();
        let has_valid_metadata = !has_keywords_error;
        let is_busy = imp.metadata.job.is_busy(imp.is_running.get());
        let preview = imp.metadata.preview.borrow();

        clear_box(&imp.metadata_preview_box);
        if has_file {
            imp.metadata_preview_box
                .append(&single_file_preview_widget(preview.as_ref()));
        }

        imp.metadata_empty_status.set_visible(!has_file);
        imp.metadata_actions.set_visible(has_file);
        imp.metadata_content.set_visible(has_file);
        imp.metadata_choose_button.set_visible(has_file);
        imp.metadata_save_button.set_visible(has_file);
        imp.metadata_advanced_options_button.set_visible(has_file);
        imp.metadata_open_output_button
            .set_visible(imp.metadata.job.has_last_output());

        imp.metadata_choose_button.set_sensitive(!is_busy);
        imp.metadata_empty_choose_button.set_sensitive(!is_busy);
        imp.metadata_save_button
            .set_sensitive(has_file && has_valid_metadata && !is_busy);
        imp.metadata_open_output_button
            .set_sensitive(imp.metadata.job.has_last_output() && !is_busy);
        imp.metadata_advanced_options_button
            .set_sensitive(has_file && !is_busy);
        self.set_entries_sensitive(has_file && !is_busy);
        self.update_keywords_entry_state(has_keywords_error);

        let detail = if imp.is_running.get() {
            gettext("Saving metadata...")
        } else if imp.metadata.job.is_loading() {
            gettext("Loading PDF...")
        } else if let Some(path) = file.as_ref() {
            file_title(path).to_string()
        } else {
            gettext("No PDF selected")
        };
        update_shell_title(self, PdfTool::Metadata, &detail);
        update_shell_view_mode(self);
    }

    fn set_entries_sensitive(&self, sensitive: bool) {
        let imp = self.imp();
        imp.metadata_title_entry.set_sensitive(sensitive);
        imp.metadata_author_entry.set_sensitive(sensitive);
        imp.metadata_subject_entry.set_sensitive(sensitive);
        imp.metadata_keywords_entry.set_sensitive(sensitive);
    }

    fn set_metadata_entries(&self, metadata: &crate::pdf::PdfDocumentMetadata) {
        let imp = self.imp();
        imp.metadata_title_entry.set_text(&metadata.title);
        imp.metadata_author_entry.set_text(&metadata.author);
        imp.metadata_subject_entry.set_text(&metadata.subject);
        imp.metadata_keywords_entry.set_text(&metadata.keywords);
        imp.metadata_creator_row
            .set_subtitle(&metadata_subtitle(&metadata.creator));
        imp.metadata_producer_row
            .set_subtitle(&metadata_subtitle(&metadata.producer));
    }

    fn metadata_from_entries(&self) -> Result<crate::pdf::PdfEditableMetadata, ()> {
        let imp = self.imp();
        Ok(crate::pdf::PdfEditableMetadata {
            title: imp.metadata_title_entry.text().to_string(),
            author: imp.metadata_author_entry.text().to_string(),
            subject: imp.metadata_subject_entry.text().to_string(),
            keywords: normalize_keywords(imp.metadata_keywords_entry.text().as_str())?,
        })
    }

    fn update_keywords_entry_state(&self, has_error: bool) {
        let imp = self.imp();

        EntryValidation::new(
            &imp.metadata_keywords_entry,
            &imp.metadata_keywords_error_row,
            &imp.metadata_keywords_error_label,
        )
        .set_error(
            imp.keywords_validation.display(has_error),
            &keywords_error_message(),
        );
    }

    fn open_last_output(&self) {
        if let Some(path) = self.imp().metadata.job.last_output() {
            open_output(self, &path);
        }
    }
}

fn metadata_subtitle(value: &str) -> String {
    if value.trim().is_empty() {
        gettext("N/A")
    } else {
        value.to_string()
    }
}

fn normalize_keywords(value: &str) -> Result<String, ()> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::new());
    }

    let keywords = value.split(',').map(str::trim).collect::<Vec<_>>();
    if keywords.iter().any(|keyword| keyword.is_empty()) {
        return Err(());
    }

    Ok(keywords.join(", "))
}

#[cfg(test)]
mod tests {
    use super::normalize_keywords;

    #[test]
    fn normalize_keywords_trims_comma_separated_values() {
        assert_eq!(normalize_keywords("").unwrap(), "");
        assert_eq!(normalize_keywords("  ").unwrap(), "");
        assert_eq!(
            normalize_keywords("alpha,beta, gamma ").unwrap(),
            "alpha, beta, gamma"
        );
    }

    #[test]
    fn normalize_keywords_rejects_empty_keywords() {
        assert!(normalize_keywords(",alpha").is_err());
        assert!(normalize_keywords("alpha,,beta").is_err());
        assert!(normalize_keywords("alpha, ").is_err());
    }
}
