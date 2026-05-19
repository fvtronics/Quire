use super::ui::{clear_box, compress_preview_widget, file_subtitle, pdf_filters};
use super::FoliosWindow;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use std::path::{Path, PathBuf};

impl FoliosWindow {
    pub(super) fn setup_compress_callbacks(&self) {
        let imp = self.imp();

        let window = self.clone();
        imp.compress_choose_button.connect_clicked(move |_| {
            window.choose_compress_file();
        });

        let window = self.clone();
        imp.compress_empty_choose_button.connect_clicked(move |_| {
            window.choose_compress_file();
        });

        let window = self.clone();
        imp.compress_save_button.connect_clicked(move |_| {
            window.choose_compress_output_file();
        });

        let window = self.clone();
        imp.compress_open_output_button.connect_clicked(move |_| {
            window.open_last_output();
        });
    }

    fn choose_compress_file(&self) {
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
                    window.load_compress_pdf(path);
                }
            }
        });
    }

    fn choose_compress_output_file(&self) {
        let Some(input_file) = self.imp().compress_file.borrow().clone() else {
            return;
        };

        let window = self.clone();
        glib::spawn_future_local(async move {
            let dialog = gtk::FileDialog::builder()
                .title(gettext("Save Compressed PDF"))
                .accept_label(gettext("Compress"))
                .initial_name("Compressed.pdf")
                .modal(true)
                .filters(&pdf_filters())
                .build();

            if let Ok(file) = dialog.save_future(Some(&window)).await {
                if let Some(path) = file.path() {
                    window.compress_to(input_file, path);
                }
            }
        });
    }

    fn load_compress_pdf(&self, path: PathBuf) {
        let imp = self.imp();
        imp.is_running.set(true);
        imp.compress_file.borrow_mut().replace(path.clone());
        imp.compress_preview.borrow_mut().take();
        imp.compress_last_output.borrow_mut().take();
        self.update_compress_view();

        let window = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::preview::render_single_file_preview(path.clone()).await;
            let imp = window.imp();
            imp.is_running.set(false);

            match result {
                Ok(preview) => {
                    if imp.compress_file.borrow().as_ref() == Some(&path) {
                        *imp.compress_preview.borrow_mut() = preview;
                    }
                }
                Err(error) => {
                    window.show_toast(&error.to_string());
                }
            }

            window.update_compress_view();
        });
    }

    fn compress_to(&self, input_file: PathBuf, output_file: PathBuf) {
        let imp = self.imp();
        let options = crate::pdf::CompressOptions {
            remove_empty_streams: imp.compress_empty_streams_row.is_active(),
            prune_objects: imp.compress_prune_row.is_active(),
        };

        imp.is_running.set(true);
        imp.compress_last_output.borrow_mut().take();
        self.update_all_views();

        let window = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::pdf::compress_pdf(input_file, output_file, options).await;
            let imp = window.imp();
            imp.is_running.set(false);

            match result {
                Ok(path) => {
                    imp.compress_last_output.borrow_mut().replace(path);
                    window.show_toast(&gettext("Compressed PDF saved"));
                }
                Err(error) => {
                    window.show_toast(&error.to_string());
                }
            }

            window.update_all_views();
        });
    }

    pub(super) fn update_compress_view(&self) {
        let imp = self.imp();
        let file = imp.compress_file.borrow();
        let has_file = file.is_some();
        let preview = imp.compress_preview.borrow();

        imp.compress_file_list.remove_all();
        clear_box(&imp.compress_preview_box);
        if let Some(path) = file.as_ref() {
            imp.compress_file_list.append(&self.compress_file_row(path));
            imp.compress_preview_box
                .append(&compress_preview_widget(preview.as_ref()));
        }

        imp.compress_empty_status.set_visible(!has_file);
        imp.compress_content.set_visible(has_file);
        imp.compress_choose_button.set_visible(has_file);
        imp.compress_save_button.set_visible(has_file);
        imp.compress_open_output_button
            .set_visible(imp.compress_last_output.borrow().is_some());

        imp.compress_choose_button
            .set_sensitive(!imp.is_running.get());
        imp.compress_empty_choose_button
            .set_sensitive(!imp.is_running.get());
        imp.compress_save_button
            .set_sensitive(has_file && !imp.is_running.get());
        imp.compress_open_output_button
            .set_sensitive(imp.compress_last_output.borrow().is_some() && !imp.is_running.get());
        imp.compress_prune_row
            .set_sensitive(has_file && !imp.is_running.get());
        imp.compress_empty_streams_row
            .set_sensitive(has_file && !imp.is_running.get());

        let detail = if imp.is_running.get() {
            gettext("Working...")
        } else if let Some(path) = file.as_ref() {
            file_subtitle(path)
        } else {
            gettext("No PDF selected")
        };
        imp.compress_detail_label.set_label(&detail);
    }

    fn compress_file_row(&self, path: &Path) -> adw::ActionRow {
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("PDF");
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle(file_subtitle(path))
            .activatable(false)
            .build();

        let icon = gtk::Image::from_icon_name("view-paged-symbolic");
        row.add_prefix(&icon);

        row
    }
}
