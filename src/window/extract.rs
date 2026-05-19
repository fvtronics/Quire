use super::ui::{
    format_page_ranges, normalize_pages, page_count_label, pdf_filters, preview_picture,
};
use super::FoliosWindow;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use std::path::{Path, PathBuf};

impl FoliosWindow {
    pub(super) fn setup_extract_callbacks(&self) {
        let imp = self.imp();

        let window = self.clone();
        imp.extract_choose_button.connect_clicked(move |_| {
            window.choose_extract_file();
        });

        let window = self.clone();
        imp.extract_empty_choose_button.connect_clicked(move |_| {
            window.choose_extract_file();
        });

        let window = self.clone();
        imp.extract_save_button.connect_clicked(move |_| {
            window.choose_extract_output_file();
        });

        let window = self.clone();
        imp.extract_open_output_button.connect_clicked(move |_| {
            window.open_last_output();
        });

        let window = self.clone();
        imp.extract_ranges_entry.connect_changed(move |entry| {
            let imp = window.imp();
            let text = entry.text();
            let text = text.trim();

            if text.is_empty() {
                imp.extract_selected_pages.borrow_mut().clear();
            } else if let Ok(pages) =
                crate::pdf::parse_page_ranges(text, imp.extract_page_count.get())
            {
                *imp.extract_selected_pages.borrow_mut() = normalize_pages(pages);
            }

            imp.extract_last_output.borrow_mut().take();
            window.update_extract_view();
        });
    }

    fn choose_extract_file(&self) {
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
                    window.load_extract_pdf(path);
                }
            }
        });
    }

    fn choose_extract_output_file(&self) {
        let imp = self.imp();
        let Some(input_file) = imp.extract_file.borrow().clone() else {
            return;
        };
        let pages = if imp.extract_ranges_entry.text().trim().is_empty() {
            let pages = imp.extract_selected_pages.borrow().clone();
            if pages.is_empty() {
                self.show_toast(&gettext("Choose at least one page to extract."));
                return;
            }
            pages
        } else {
            match self.extract_pages_from_ranges() {
                Ok(pages) => pages,
                Err(error) => {
                    self.show_toast(&error.to_string());
                    return;
                }
            }
        };

        let window = self.clone();
        glib::spawn_future_local(async move {
            let dialog = gtk::FileDialog::builder()
                .title(gettext("Save Extracted Pages"))
                .accept_label(gettext("Extract"))
                .initial_name("Extracted.pdf")
                .modal(true)
                .filters(&pdf_filters())
                .build();

            if let Ok(file) = dialog.save_future(Some(&window)).await {
                if let Some(path) = file.path() {
                    window.extract_to(input_file, pages, path);
                }
            }
        });
    }

    fn load_extract_pdf(&self, path: PathBuf) {
        let imp = self.imp();
        imp.is_running.set(true);
        imp.extract_last_output.borrow_mut().take();
        self.update_extract_view();

        let window = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::preview::render_page_previews(path.clone()).await;
            let imp = window.imp();
            imp.is_running.set(false);

            match result {
                Ok(previews) => {
                    let page_count = previews.len();
                    imp.extract_file.borrow_mut().replace(path);
                    imp.extract_page_count.set(page_count);
                    *imp.extract_previews.borrow_mut() = previews;
                    imp.extract_selected_pages.borrow_mut().clear();
                    imp.extract_ranges_entry.set_text("");
                }
                Err(error) => {
                    window.show_toast(&error.to_string());
                }
            }

            window.update_extract_view();
        });
    }

    fn extract_to(&self, input_file: PathBuf, pages: Vec<u32>, output_file: PathBuf) {
        let imp = self.imp();
        imp.is_running.set(true);
        imp.extract_last_output.borrow_mut().take();
        self.update_all_views();

        let window = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::pdf::extract_pages(input_file, pages, output_file).await;
            let imp = window.imp();
            imp.is_running.set(false);

            match result {
                Ok(path) => {
                    imp.extract_last_output.borrow_mut().replace(path);
                    window.show_toast(&gettext("Extracted pages saved"));
                }
                Err(error) => {
                    window.show_toast(&error.to_string());
                }
            }

            window.update_all_views();
        });
    }

    pub(super) fn update_extract_view(&self) {
        let imp = self.imp();
        let has_file = imp.extract_file.borrow().is_some();
        let has_ranges = !imp.extract_ranges_entry.text().trim().is_empty();
        let has_valid_ranges = has_ranges && self.extract_pages_from_ranges().is_ok();
        let has_selected_pages = !imp.extract_selected_pages.borrow().is_empty();

        imp.extract_file_list.remove_all();
        if let Some(path) = imp.extract_file.borrow().as_ref() {
            imp.extract_file_list
                .append(&self.extract_file_row(path, imp.extract_page_count.get()));
        }

        imp.extract_page_list.remove_all();
        imp.extract_page_grid.remove_all();
        let selected_pages = imp.extract_selected_pages.borrow();
        let previews = imp.extract_previews.borrow();
        for page_number in 1..=imp.extract_page_count.get() as u32 {
            let preview = previews
                .iter()
                .find(|preview| preview.page_number == page_number);
            imp.extract_page_list.append(&self.extract_page_row(
                page_number,
                selected_pages.contains(&page_number),
                preview,
            ));
        }
        for preview in previews.iter() {
            imp.extract_page_grid.append(
                &self.extract_page_tile(preview, selected_pages.contains(&preview.page_number)),
            );
        }

        imp.extract_empty_status.set_visible(!has_file);
        imp.extract_content.set_visible(has_file);
        imp.extract_choose_button.set_visible(has_file);
        imp.extract_save_button.set_visible(has_file);
        imp.extract_open_output_button
            .set_visible(imp.extract_last_output.borrow().is_some());

        imp.extract_choose_button
            .set_sensitive(!imp.is_running.get());
        imp.extract_empty_choose_button
            .set_sensitive(!imp.is_running.get());
        imp.extract_save_button.set_sensitive(
            has_file
                && (has_valid_ranges || (!has_ranges && has_selected_pages))
                && !imp.is_running.get(),
        );
        imp.extract_open_output_button
            .set_sensitive(imp.extract_last_output.borrow().is_some() && !imp.is_running.get());
        imp.extract_ranges_entry
            .set_sensitive(has_file && !imp.is_running.get());

        let detail = if imp.is_running.get() {
            gettext("Working...")
        } else if has_file {
            page_count_label(imp.extract_page_count.get())
        } else {
            gettext("No PDF selected")
        };
        imp.extract_detail_label.set_label(&detail);
    }

    fn extract_pages_from_ranges(&self) -> Result<Vec<u32>, crate::pdf::PdfBackendError> {
        let imp = self.imp();
        crate::pdf::parse_page_ranges(
            imp.extract_ranges_entry.text().as_str(),
            imp.extract_page_count.get(),
        )
    }

    fn extract_file_row(&self, path: &Path, page_count: usize) -> adw::ActionRow {
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("PDF");
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle(page_count_label(page_count))
            .activatable(false)
            .build();

        let icon = gtk::Image::from_icon_name("view-paged-symbolic");
        row.add_prefix(&icon);
        row
    }

    fn extract_page_row(
        &self,
        page_number: u32,
        selected: bool,
        preview: Option<&crate::preview::PagePreview>,
    ) -> adw::ActionRow {
        let check_button = gtk::CheckButton::builder()
            .active(selected)
            .tooltip_text(gettext("Select Page"))
            .valign(gtk::Align::Center)
            .build();
        let row = adw::ActionRow::builder()
            .title(format!("{} {page_number}", gettext("Page")))
            .activatable(true)
            .activatable_widget(&check_button)
            .build();

        if let Some(preview) = preview {
            let picture = preview_picture(preview);
            picture.set_size_request(48, 68);
            row.add_prefix(&picture);
        } else {
            let icon = gtk::Image::from_icon_name("view-paged-symbolic");
            row.add_prefix(&icon);
        }
        row.add_suffix(&check_button);

        let window = self.clone();
        check_button.connect_toggled(move |button| {
            window.toggle_extract_page(page_number, button.is_active());
        });

        row
    }

    fn extract_page_tile(
        &self,
        preview: &crate::preview::PagePreview,
        selected: bool,
    ) -> gtk::ToggleButton {
        let button = gtk::ToggleButton::builder()
            .active(selected)
            .tooltip_text(gettext("Select Page"))
            .width_request(180)
            .build();
        button.set_css_classes(&["flat"]);

        let tile = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .build();

        let picture = preview_picture(preview);
        picture.set_size_request(160, 220);
        tile.append(&picture);

        let footer = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        let label = gtk::Label::builder()
            .label(format!("{} {}", gettext("Page"), preview.page_number))
            .xalign(0.0)
            .hexpand(true)
            .build();
        let check_icon = gtk::Image::from_icon_name("object-select-symbolic");
        check_icon.set_opacity(if selected { 1.0 } else { 0.0 });

        footer.append(&label);
        footer.append(&check_icon);
        tile.append(&footer);

        button.set_child(Some(&tile));

        let window = self.clone();
        let page_number = preview.page_number;
        button.connect_toggled(move |button| {
            window.toggle_extract_page(page_number, button.is_active());
        });

        button
    }

    fn toggle_extract_page(&self, page_number: u32, selected: bool) {
        let imp = self.imp();
        let mut pages = imp.extract_selected_pages.borrow_mut();

        if selected {
            if !pages.contains(&page_number) {
                pages.push(page_number);
                pages.sort_unstable();
            }
        } else {
            pages.retain(|page| *page != page_number);
        }

        imp.extract_last_output.borrow_mut().take();
        drop(pages);

        self.update_extract_ranges_entry();

        self.update_extract_view();
    }

    fn update_extract_ranges_entry(&self) {
        let imp = self.imp();
        let text = {
            let pages = imp.extract_selected_pages.borrow();
            format_page_ranges(&pages)
        };

        if imp.extract_ranges_entry.text().as_str() != text {
            imp.extract_ranges_entry.set_text(&text);
        }
    }
}
