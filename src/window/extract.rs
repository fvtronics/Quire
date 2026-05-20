use super::ui::{
    format_page_ranges, icon_button, normalize_pages, open_pdf_file, page_count_label,
    pdf_file_row, preview_tile, rotated_list_preview_prefix, save_pdf_file, tile_controls,
    tile_label, tile_preview_widget,
};
use super::FoliosWindow;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use std::path::PathBuf;

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
                imp.extract.clear_range_selection();
            } else if let Ok(pages) =
                crate::pdf::parse_page_ranges(text, imp.extract.page_count.get())
            {
                let pages = normalize_pages(pages);
                imp.extract.apply_range_selection(pages);
            }

            imp.extract.clear_last_output();
            window.update_extract_view();
        });
    }

    fn choose_extract_file(&self) {
        let window = self.clone();
        glib::spawn_future_local(async move {
            if let Some(path) = open_pdf_file(&window, &gettext("Open PDF"), &gettext("Open")).await
            {
                window.load_extract_pdf(path);
            }
        });
    }

    fn choose_extract_output_file(&self) {
        let imp = self.imp();
        let page_numbers = if imp.extract_ranges_entry.text().trim().is_empty() {
            let pages = imp.extract.selected_pages.borrow().clone();
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
        let Some((input_file, pages)) = imp.extract.selections_from_pages(page_numbers) else {
            return;
        };

        let window = self.clone();
        glib::spawn_future_local(async move {
            if let Some(path) = save_pdf_file(
                &window,
                &gettext("Save Extracted Pages"),
                &gettext("Extract"),
                "Extracted.pdf",
            )
            .await
            {
                window.extract_to(input_file, pages, path);
            }
        });
    }

    fn load_extract_pdf(&self, path: PathBuf) {
        let imp = self.imp();
        imp.is_running.set(true);
        imp.extract.clear_last_output();
        self.update_extract_view();

        let window = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::preview::render_page_previews(path.clone()).await;
            let imp = window.imp();
            imp.is_running.set(false);

            match result {
                Ok(previews) => {
                    imp.extract.load_document(path, previews);
                    imp.extract_ranges_entry.set_text("");
                }
                Err(error) => {
                    window.show_toast(&error.to_string());
                }
            }

            window.update_extract_view();
        });
    }

    fn extract_to(
        &self,
        input_file: PathBuf,
        pages: Vec<crate::pdf::PageSelection>,
        output_file: PathBuf,
    ) {
        let imp = self.imp();
        imp.is_running.set(true);
        imp.extract.clear_last_output();
        self.update_all_views();

        let window = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::pdf::extract_pages(input_file, pages, output_file).await;
            let imp = window.imp();
            imp.is_running.set(false);

            match result {
                Ok(path) => {
                    imp.extract.set_last_output(path);
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
        self.update_view_mode();

        let imp = self.imp();
        let has_file = imp.extract.file.borrow().is_some();
        let has_ranges = !imp.extract_ranges_entry.text().trim().is_empty();
        let has_valid_ranges = has_ranges && self.extract_pages_from_ranges().is_ok();
        let has_selected_pages = !imp.extract.selected_pages.borrow().is_empty();

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
            let preview = previews
                .iter()
                .find(|preview| preview.page_number == page_number);
            let selected = selected_pages.contains(&page_number);
            let rotation = *rotations.get(&page_number).unwrap_or(&0);
            imp.extract_page_list.append(&self.extract_page_row(
                page_number,
                selected,
                preview,
                rotation,
            ));
        }
        for preview in previews.iter() {
            let selected = selected_pages.contains(&preview.page_number);
            let rotation = *rotations.get(&preview.page_number).unwrap_or(&0);
            imp.extract_page_grid
                .append(&self.extract_page_tile(preview, selected, rotation));
        }

        imp.extract_empty_status.set_visible(!has_file);
        imp.extract_content.set_visible(has_file);
        imp.extract_choose_button.set_visible(has_file);
        imp.extract_save_button.set_visible(has_file);
        imp.extract_open_output_button
            .set_visible(imp.extract.last_output.borrow().is_some());

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
            .set_sensitive(imp.extract.last_output.borrow().is_some() && !imp.is_running.get());
        imp.extract_ranges_entry
            .set_sensitive(has_file && !imp.is_running.get());

        let detail = if imp.is_running.get() {
            gettext("Extracting pages...")
        } else if has_file {
            page_count_label(imp.extract.page_count.get())
        } else {
            gettext("No PDF selected")
        };
        imp.extract_detail_label.set_label(&detail);
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
            .activatable(true)
            .activatable_widget(&check_button)
            .build();

        row.add_prefix(&rotated_list_preview_prefix(preview, rotation));

        let rotate_button =
            icon_button("object-rotate-right-symbolic", &gettext("Rotate Clockwise"));
        rotate_button.set_sensitive(selected && !self.imp().is_running.get());
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
        preview: &crate::preview::PagePreview,
        selected: bool,
        rotation: i64,
    ) -> gtk::Box {
        let tile = preview_tile();
        tile.append(&tile_preview_widget(Some(preview), rotation));

        let footer = tile_controls();
        let label = tile_label(format!("{} {}", gettext("Page"), preview.page_number));
        label.set_hexpand(true);
        let rotate_button =
            icon_button("object-rotate-right-symbolic", &gettext("Rotate Clockwise"));
        rotate_button.set_sensitive(selected && !self.imp().is_running.get());
        let page_number = preview.page_number;
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
        let page_number = preview.page_number;
        check_button.connect_toggled(move |button| {
            window.toggle_extract_page(page_number, button.is_active());
        });

        tile
    }

    fn toggle_extract_page(&self, page_number: u32, selected: bool) {
        self.imp().extract.toggle_page(page_number, selected);
        self.update_extract_ranges_entry();
        self.update_extract_view();
    }

    fn rotate_extract_page(&self, page_number: u32) {
        if self.imp().extract.rotate_page(page_number) {
            self.update_extract_view();
        }
    }

    fn update_extract_ranges_entry(&self) {
        let imp = self.imp();
        let text = {
            let pages = imp.extract.selected_pages.borrow();
            format_page_ranges(&pages)
        };

        if imp.extract_ranges_entry.text().as_str() != text {
            imp.extract_ranges_entry.set_text(&text);
        }
    }
}
