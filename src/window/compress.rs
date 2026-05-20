use super::ui::{
    clear_box, file_subtitle, open_pdf_file, pdf_file_row, save_pdf_file,
    single_file_preview_widget,
};
use super::FoliosWindow;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use std::path::PathBuf;

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
            if let Some(path) = open_pdf_file(&window, &gettext("Open PDF"), &gettext("Open")).await
            {
                window.load_compress_pdf(path);
            }
        });
    }

    fn choose_compress_output_file(&self) {
        let Some(input_file) = self.imp().compress.file.borrow().clone() else {
            return;
        };

        let window = self.clone();
        glib::spawn_future_local(async move {
            if let Some(path) = save_pdf_file(
                &window,
                &gettext("Save Compressed PDF"),
                &gettext("Compress"),
                "Compressed.pdf",
            )
            .await
            {
                window.compress_to(input_file, path);
            }
        });
    }

    fn load_compress_pdf(&self, path: PathBuf) {
        let imp = self.imp();
        imp.is_running.set(true);
        imp.compress.file.borrow_mut().replace(path.clone());
        imp.compress.preview.borrow_mut().take();
        imp.compress.last_output.borrow_mut().take();
        self.update_compress_view();

        let window = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::preview::render_single_file_preview(path.clone()).await;
            let imp = window.imp();
            imp.is_running.set(false);

            match result {
                Ok(preview) => {
                    if imp.compress.file.borrow().as_ref() == Some(&path) {
                        *imp.compress.preview.borrow_mut() = preview;
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
        imp.compress.last_output.borrow_mut().take();
        self.update_all_views();

        let window = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::pdf::compress_pdf(input_file, output_file, options).await;
            let imp = window.imp();
            imp.is_running.set(false);

            match result {
                Ok(path) => {
                    imp.compress.last_output.borrow_mut().replace(path);
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
        let file = imp.compress.file.borrow();
        let has_file = file.is_some();
        let preview = imp.compress.preview.borrow();

        imp.compress_file_list.remove_all();
        clear_box(&imp.compress_preview_box);
        if let Some(path) = file.as_ref() {
            imp.compress_file_list
                .append(&pdf_file_row(path, file_subtitle(path)));
            imp.compress_preview_box
                .append(&single_file_preview_widget(preview.as_ref()));
        }

        imp.compress_empty_status.set_visible(!has_file);
        imp.compress_content.set_visible(has_file);
        imp.compress_choose_button.set_visible(has_file);
        imp.compress_save_button.set_visible(has_file);
        imp.compress_open_output_button
            .set_visible(imp.compress.last_output.borrow().is_some());

        imp.compress_choose_button
            .set_sensitive(!imp.is_running.get());
        imp.compress_empty_choose_button
            .set_sensitive(!imp.is_running.get());
        imp.compress_save_button
            .set_sensitive(has_file && !imp.is_running.get());
        imp.compress_open_output_button
            .set_sensitive(imp.compress.last_output.borrow().is_some() && !imp.is_running.get());
        imp.compress_prune_row
            .set_sensitive(has_file && !imp.is_running.get());
        imp.compress_empty_streams_row
            .set_sensitive(has_file && !imp.is_running.get());

        let detail = if imp.is_running.get() {
            gettext("Compressing PDF...")
        } else if let Some(path) = file.as_ref() {
            file_subtitle(path)
        } else {
            gettext("No PDF selected")
        };
        imp.compress_detail_label.set_label(&detail);
    }
}
