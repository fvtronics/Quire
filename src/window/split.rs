use super::ui::{clear_box, compress_preview_widget, file_subtitle, page_count_label, pdf_filters};
use super::FoliosWindow;
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

impl FoliosWindow {
    pub(super) fn setup_split_callbacks(&self) {
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

        let window = self.clone();
        imp.split_choose_button.connect_clicked(move |_| {
            window.choose_split_file();
        });

        let window = self.clone();
        imp.split_empty_choose_button.connect_clicked(move |_| {
            window.choose_split_file();
        });

        let window = self.clone();
        imp.split_save_button.connect_clicked(move |_| {
            window.choose_split_output_folder();
        });

        let window = self.clone();
        imp.split_open_output_button.connect_clicked(move |_| {
            window.open_last_output();
        });

        let window = self.clone();
        imp.split_after_row.connect_selected_notify(move |_| {
            window.imp().split_last_output.borrow_mut().take();
            window.update_split_view();
        });

        let window = self.clone();
        imp.split_specific_pages_entry.connect_changed(move |_| {
            window.imp().split_last_output.borrow_mut().take();
            window.update_split_view();
        });

        let window = self.clone();
        imp.split_pages_entry.connect_changed(move |_| {
            window.imp().split_last_output.borrow_mut().take();
            window.update_split_view();
        });

        let window = self.clone();
        imp.split_prefix_entry.connect_changed(move |_| {
            window.imp().split_last_output.borrow_mut().take();
            window.update_split_view();
        });
    }

    fn choose_split_file(&self) {
        let window = self.clone();
        glib::spawn_future_local(async move {
            let dialog = gtk::FileDialog::builder()
                .title(gettext("Open PDF"))
                .accept_label(gettext("Open"))
                .modal(true)
                .filters(&pdf_filters())
                .build();

            if let Ok(file) = dialog.open_future(Some(&window)).await {
                if let Some(path) = file.path() {
                    window.load_split_pdf(path);
                }
            }
        });
    }

    fn choose_split_output_folder(&self) {
        let imp = self.imp();
        let Some(input_file) = imp.split_file.borrow().clone() else {
            return;
        };
        let rule = match self.split_rule() {
            Ok(rule) => rule,
            Err(error) => {
                self.show_toast(&error.to_string());
                return;
            }
        };
        let prefix = imp.split_prefix_entry.text().to_string();

        let window = self.clone();
        glib::spawn_future_local(async move {
            let dialog = gtk::FileDialog::builder()
                .title(gettext("Choose Output Folder"))
                .accept_label(gettext("Split"))
                .modal(true)
                .build();

            if let Ok(folder) = dialog.select_folder_future(Some(&window)).await {
                if let Some(path) = folder.path() {
                    window.split_to(input_file, path, prefix, rule);
                }
            }
        });
    }

    fn load_split_pdf(&self, path: PathBuf) {
        let imp = self.imp();
        imp.is_running.set(true);
        imp.split_file.borrow_mut().replace(path.clone());
        imp.split_page_count.set(0);
        imp.split_preview.borrow_mut().take();
        imp.split_last_output.borrow_mut().take();
        imp.split_prefix_entry
            .set_text(&split_default_prefix(&path));
        self.update_split_view();

        let window = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::preview::render_first_page_preview_with_count(path.clone()).await;
            let imp = window.imp();
            imp.is_running.set(false);

            match result {
                Ok((preview, page_count)) => {
                    if imp.split_file.borrow().as_ref() == Some(&path) {
                        imp.split_page_count.set(page_count);
                        *imp.split_preview.borrow_mut() = preview;
                    }
                }
                Err(error) => {
                    window.show_toast(&error.to_string());
                }
            }

            window.update_split_view();
        });
    }

    fn split_to(
        &self,
        input_file: PathBuf,
        output_folder: PathBuf,
        prefix: String,
        rule: crate::pdf::SplitRule,
    ) {
        let imp = self.imp();
        imp.is_running.set(true);
        imp.split_last_output.borrow_mut().take();
        self.update_all_views();

        let window = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::pdf::split_pdf(input_file, output_folder, prefix, rule).await;
            let imp = window.imp();
            imp.is_running.set(false);

            match result {
                Ok(path) => {
                    imp.split_last_output.borrow_mut().replace(path);
                    window.show_toast(&gettext("Split PDFs saved"));
                }
                Err(error) => {
                    window.show_toast(&error.to_string());
                }
            }

            window.update_all_views();
        });
    }

    pub(super) fn update_split_view(&self) {
        let imp = self.imp();
        let file = imp.split_file.borrow();
        let has_file = file.is_some();
        let has_split_rule = self.split_rule().is_ok();
        let split_mode = imp.split_after_row.selected();
        let preview = imp.split_preview.borrow();

        imp.split_file_list.remove_all();
        clear_box(&imp.split_preview_box);
        if let Some(path) = file.as_ref() {
            imp.split_file_list
                .append(&self.split_file_row(path, imp.split_page_count.get()));
            imp.split_preview_box
                .append(&compress_preview_widget(preview.as_ref()));
        }

        imp.split_empty_status.set_visible(!has_file);
        imp.split_content.set_visible(has_file);
        imp.split_choose_button.set_visible(has_file);
        imp.split_save_button.set_visible(has_file);
        imp.split_open_output_button
            .set_visible(imp.split_last_output.borrow().is_some());

        imp.split_choose_button.set_sensitive(!imp.is_running.get());
        imp.split_empty_choose_button
            .set_sensitive(!imp.is_running.get());
        imp.split_save_button
            .set_sensitive(has_file && has_split_rule && !imp.is_running.get());
        imp.split_open_output_button
            .set_sensitive(imp.split_last_output.borrow().is_some() && !imp.is_running.get());
        imp.split_after_row
            .set_sensitive(has_file && !imp.is_running.get());
        imp.split_specific_pages_entry
            .set_visible(split_mode == SPLIT_SPECIFIC_PAGES);
        imp.split_specific_pages_entry
            .set_sensitive(has_file && split_mode == SPLIT_SPECIFIC_PAGES && !imp.is_running.get());
        imp.split_pages_entry
            .set_visible(split_mode == SPLIT_EVERY_N_PAGES);
        imp.split_pages_entry
            .set_sensitive(has_file && split_mode == SPLIT_EVERY_N_PAGES && !imp.is_running.get());
        imp.split_prefix_entry
            .set_sensitive(has_file && !imp.is_running.get());

        let detail = if imp.is_running.get() {
            gettext("Working...")
        } else if has_file {
            page_count_label(imp.split_page_count.get())
        } else {
            gettext("No PDF selected")
        };
        imp.split_detail_label.set_label(&detail);
    }

    fn split_rule(&self) -> Result<crate::pdf::SplitRule, crate::pdf::PdfBackendError> {
        let imp = self.imp();
        match imp.split_after_row.selected() {
            SPLIT_EVERY_PAGE => Ok(crate::pdf::SplitRule::EveryPage),
            SPLIT_EVEN_PAGES => Ok(crate::pdf::SplitRule::EvenPages),
            SPLIT_ODD_PAGES => Ok(crate::pdf::SplitRule::OddPages),
            SPLIT_SPECIFIC_PAGES => {
                let pages = crate::pdf::parse_page_numbers(
                    imp.split_specific_pages_entry.text().as_str(),
                    imp.split_page_count.get(),
                )?;
                Ok(crate::pdf::SplitRule::SpecificPages(pages))
            }
            SPLIT_EVERY_N_PAGES => {
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
            _ => Ok(crate::pdf::SplitRule::EveryPage),
        }
    }

    fn split_file_row(&self, path: &Path, page_count: usize) -> adw::ActionRow {
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("PDF");
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle(format!(
                "{} - {}",
                page_count_label(page_count),
                file_subtitle(path)
            ))
            .activatable(false)
            .build();

        let icon = gtk::Image::from_icon_name("view-paged-symbolic");
        row.add_prefix(&icon);

        row
    }
}

fn split_default_prefix(path: &Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("Split")
        .to_string()
}
