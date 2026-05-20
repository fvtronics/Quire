use super::ui::{
    clear_box, file_subtitle, open_pdf_file, page_count_label, pdf_file_row, select_folder,
    single_file_preview_widget,
};
use super::workspace::{
    open_output, parent_window, run_output_job, show_backend_error, show_preview_error,
    update_shell_view_mode,
};
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use std::path::{Path, PathBuf};

const SPLIT_EVERY_PAGE: u32 = 0;
const SPLIT_EVEN_PAGES: u32 = 1;
const SPLIT_ODD_PAGES: u32 = 2;
const SPLIT_SPECIFIC_PAGES: u32 = 3;
const SPLIT_EVERY_N_PAGES: u32 = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SplitMode {
    EveryPage,
    EvenPages,
    OddPages,
    SpecificPages,
    EveryNPages,
}

impl SplitMode {
    fn from_index(index: u32) -> Option<Self> {
        match index {
            SPLIT_EVERY_PAGE => Some(Self::EveryPage),
            SPLIT_EVEN_PAGES => Some(Self::EvenPages),
            SPLIT_ODD_PAGES => Some(Self::OddPages),
            SPLIT_SPECIFIC_PAGES => Some(Self::SpecificPages),
            SPLIT_EVERY_N_PAGES => Some(Self::EveryNPages),
            _ => None,
        }
    }
}

mod imp {
    use super::super::state::SplitState;
    use adw::subclass::prelude::*;
    use gtk::{glib, TemplateChild};
    use std::cell::Cell;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/fvtronics/folios/split-workspace.ui")]
    pub struct SplitWorkspace {
        #[template_child]
        pub split_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub split_empty_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub split_detail_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub split_empty_status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub split_content: TemplateChild<gtk::Box>,
        #[template_child]
        pub split_preview_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub split_file_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub split_after_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub split_specific_pages_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub split_pages_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub split_prefix_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub split_save_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub split_open_output_button: TemplateChild<gtk::Button>,

        pub split: SplitState,
        pub is_running: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SplitWorkspace {
        const NAME: &'static str = "SplitWorkspace";
        type Type = super::SplitWorkspace;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SplitWorkspace {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_callbacks();
            obj.update_view();
        }
    }
    impl WidgetImpl for SplitWorkspace {}
    impl BoxImpl for SplitWorkspace {}
}

glib::wrapper! {
    pub struct SplitWorkspace(ObjectSubclass<imp::SplitWorkspace>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl SplitWorkspace {
    fn setup_callbacks(&self) {
        let imp = self.imp();

        let split_after_options = [
            gettext("Every Page"),
            gettext("Even Pages"),
            gettext("Odd Pages"),
            gettext("Specific Pages"),
            gettext("Every N Pages"),
        ];
        let split_after_options = split_after_options
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        imp.split_after_row
            .set_model(Some(&gtk::StringList::new(&split_after_options)));
        imp.split_after_row
            .set_expression(Some(gtk::PropertyExpression::new(
                gtk::StringObject::static_type(),
                gtk::Expression::NONE,
                "string",
            )));

        let workspace = self.clone();
        imp.split_choose_button.connect_clicked(move |_| {
            workspace.choose_file();
        });

        let workspace = self.clone();
        imp.split_empty_choose_button.connect_clicked(move |_| {
            workspace.choose_file();
        });

        let workspace = self.clone();
        imp.split_save_button.connect_clicked(move |_| {
            workspace.choose_output_folder();
        });

        let workspace = self.clone();
        imp.split_open_output_button.connect_clicked(move |_| {
            workspace.open_last_output();
        });

        let workspace = self.clone();
        imp.split_after_row.connect_selected_notify(move |_| {
            workspace.imp().split.job.clear_last_output();
            workspace.update_view();
        });

        let workspace = self.clone();
        imp.split_specific_pages_entry.connect_changed(move |_| {
            workspace.imp().split.job.clear_last_output();
            workspace.update_view();
        });

        let workspace = self.clone();
        imp.split_pages_entry.connect_changed(move |_| {
            workspace.imp().split.job.clear_last_output();
            workspace.update_view();
        });

        let workspace = self.clone();
        imp.split_prefix_entry.connect_changed(move |_| {
            workspace.imp().split.job.clear_last_output();
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
                workspace.load_pdf(path);
            }
        });
    }

    fn choose_output_folder(&self) {
        let imp = self.imp();
        let Some(input_file) = imp.split.input_file() else {
            return;
        };
        let rule = match self.split_rule() {
            Ok(rule) => rule,
            Err(error) => {
                show_backend_error(self, &error);
                return;
            }
        };
        let prefix = imp.split_prefix_entry.text().to_string();
        let Some(parent) = parent_window(self) else {
            return;
        };

        let workspace = self.clone();
        glib::spawn_future_local(async move {
            if let Some(path) =
                select_folder(&parent, &gettext("Choose Output Folder"), &gettext("Split")).await
            {
                workspace.split_to(input_file, path, prefix, rule);
            }
        });
    }

    fn load_pdf(&self, path: PathBuf) {
        let imp = self.imp();
        imp.split.job.begin_loading();
        self.update_view();

        let workspace = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::preview::render_first_page_preview_with_count(path.clone()).await;
            let imp = workspace.imp();

            match result {
                Ok((preview, page_count)) => {
                    imp.split.finish_loading(path.clone(), preview, page_count);
                    imp.split_prefix_entry
                        .set_text(&split_default_prefix(&path));
                }
                Err(error) => {
                    imp.split.job.finish_loading_failed();
                    show_preview_error(&workspace, &error);
                }
            }

            workspace.update_view();
        });
    }

    fn split_to(
        &self,
        input_file: PathBuf,
        output_folder: PathBuf,
        prefix: String,
        rule: crate::pdf::SplitRule,
    ) {
        run_output_job(
            self.clone(),
            crate::pdf::split_pdf(input_file, output_folder, prefix, rule),
            gettext("Split PDFs saved"),
            |workspace, running| workspace.imp().is_running.set(running),
            |workspace| workspace.imp().split.job.clear_last_output(),
            |workspace, path| workspace.imp().split.job.set_last_output(path),
            Self::update_view,
        );
    }

    pub(super) fn update_view(&self) {
        let imp = self.imp();
        let file = imp.split.file.borrow();
        let has_file = file.is_some();
        let has_split_rule = self.split_rule().is_ok();
        let is_busy = imp.split.job.is_busy(imp.is_running.get());
        let split_mode = SplitMode::from_index(imp.split_after_row.selected());
        let preview = imp.split.preview.borrow();

        imp.split_file_list.remove_all();
        clear_box(&imp.split_preview_box);
        if let Some(path) = file.as_ref() {
            imp.split_file_list
                .append(&self.split_file_row(path, imp.split.page_count.get()));
            imp.split_preview_box
                .append(&single_file_preview_widget(preview.as_ref()));
        }

        imp.split_empty_status.set_visible(!has_file);
        imp.split_content.set_visible(has_file);
        imp.split_choose_button.set_visible(has_file);
        imp.split_save_button.set_visible(has_file);
        imp.split_open_output_button
            .set_visible(imp.split.job.has_last_output());

        imp.split_choose_button.set_sensitive(!is_busy);
        imp.split_empty_choose_button.set_sensitive(!is_busy);
        imp.split_save_button
            .set_sensitive(has_file && has_split_rule && !is_busy);
        imp.split_open_output_button
            .set_sensitive(imp.split.job.has_last_output() && !is_busy);
        imp.split_after_row.set_sensitive(has_file && !is_busy);
        imp.split_specific_pages_entry
            .set_visible(split_mode == Some(SplitMode::SpecificPages));
        imp.split_specific_pages_entry
            .set_sensitive(has_file && split_mode == Some(SplitMode::SpecificPages) && !is_busy);
        imp.split_pages_entry
            .set_visible(split_mode == Some(SplitMode::EveryNPages));
        imp.split_pages_entry
            .set_sensitive(has_file && split_mode == Some(SplitMode::EveryNPages) && !is_busy);
        imp.split_prefix_entry.set_sensitive(has_file && !is_busy);

        let detail = if imp.is_running.get() {
            gettext("Splitting PDF...")
        } else if imp.split.job.is_loading() {
            gettext("Loading PDF...")
        } else if has_file {
            page_count_label(imp.split.page_count.get())
        } else {
            gettext("No PDF selected")
        };
        imp.split_detail_label.set_label(&detail);
        update_shell_view_mode(self);
    }

    fn split_rule(&self) -> Result<crate::pdf::SplitRule, crate::pdf::PdfBackendError> {
        let imp = self.imp();
        match SplitMode::from_index(imp.split_after_row.selected()) {
            Some(SplitMode::EveryPage) => Ok(crate::pdf::SplitRule::EveryPage),
            Some(SplitMode::EvenPages) => Ok(crate::pdf::SplitRule::EvenPages),
            Some(SplitMode::OddPages) => Ok(crate::pdf::SplitRule::OddPages),
            Some(SplitMode::SpecificPages) => {
                let pages = crate::pdf::parse_page_numbers(
                    imp.split_specific_pages_entry.text().as_str(),
                    imp.split.page_count.get(),
                )?;
                Ok(crate::pdf::SplitRule::SpecificPages(pages))
            }
            Some(SplitMode::EveryNPages) => {
                let pages = imp
                    .split_pages_entry
                    .text()
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| {
                        crate::pdf::PdfBackendError::InvalidPageRange(
                            "Enter a page count of 1 or more.".to_string(),
                        )
                    })?;

                if pages == 0 {
                    Err(crate::pdf::PdfBackendError::InvalidPageRange(
                        "Enter a page count of 1 or more.".to_string(),
                    ))
                } else {
                    Ok(crate::pdf::SplitRule::EveryNPages(pages))
                }
            }
            None => Err(crate::pdf::PdfBackendError::InvalidPageRange(
                "Choose how to split this PDF.".to_string(),
            )),
        }
    }

    fn split_file_row(&self, path: &Path, page_count: usize) -> adw::ActionRow {
        pdf_file_row(
            path,
            format!("{} - {}", page_count_label(page_count), file_subtitle(path)),
        )
    }

    fn open_last_output(&self) {
        if let Some(path) = self.imp().split.job.last_output() {
            open_output(self, &path);
        }
    }
}

fn split_default_prefix(path: &Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("Split")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        SplitMode, SPLIT_EVEN_PAGES, SPLIT_EVERY_N_PAGES, SPLIT_EVERY_PAGE, SPLIT_ODD_PAGES,
        SPLIT_SPECIFIC_PAGES,
    };

    #[test]
    fn split_mode_maps_known_indices() {
        assert_eq!(
            SplitMode::from_index(SPLIT_EVERY_PAGE),
            Some(SplitMode::EveryPage)
        );
        assert_eq!(
            SplitMode::from_index(SPLIT_EVEN_PAGES),
            Some(SplitMode::EvenPages)
        );
        assert_eq!(
            SplitMode::from_index(SPLIT_ODD_PAGES),
            Some(SplitMode::OddPages)
        );
        assert_eq!(
            SplitMode::from_index(SPLIT_SPECIFIC_PAGES),
            Some(SplitMode::SpecificPages)
        );
        assert_eq!(
            SplitMode::from_index(SPLIT_EVERY_N_PAGES),
            Some(SplitMode::EveryNPages)
        );
    }

    #[test]
    fn split_mode_rejects_unknown_indices() {
        assert_eq!(SplitMode::from_index(99), None);
    }
}
