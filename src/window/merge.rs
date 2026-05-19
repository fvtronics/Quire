use super::ui::{
    file_subtitle, file_title, icon_button, list_preview_prefix, pdf_filters, preview_picture,
};
use super::FoliosWindow;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::{gio, glib};
use std::path::{Path, PathBuf};

impl FoliosWindow {
    pub(super) fn setup_merge_callbacks(&self) {
        let imp = self.imp();

        let window = self.clone();
        imp.add_button.connect_clicked(move |_| {
            window.choose_pdf_files();
        });

        let window = self.clone();
        imp.empty_add_button.connect_clicked(move |_| {
            window.choose_pdf_files();
        });

        let window = self.clone();
        imp.clear_button.connect_clicked(move |_| {
            let imp = window.imp();
            imp.input_files.borrow_mut().clear();
            imp.merge_previews.borrow_mut().clear();
            imp.last_output.borrow_mut().take();
            window.update_files_view();
        });

        let window = self.clone();
        imp.merge_button.connect_clicked(move |_| {
            window.choose_output_file();
        });

        let window = self.clone();
        imp.open_output_button.connect_clicked(move |_| {
            window.open_last_output();
        });
    }

    fn choose_pdf_files(&self) {
        let window = self.clone();
        glib::spawn_future_local(async move {
            let dialog = gtk::FileDialog::builder()
                .title(gettext("Add PDFs"))
                .accept_label(gettext("Add"))
                .modal(true)
                .filters(&pdf_filters())
                .build();

            if let Ok(files) = dialog.open_multiple_future(Some(&window)).await {
                let mut paths = Vec::new();
                for position in 0..files.n_items() {
                    if let Some(file) = files.item(position).and_downcast::<gio::File>() {
                        if let Some(path) = file.path() {
                            paths.push(path);
                        }
                    }
                }
                window.add_files(paths);
            }
        });
    }

    fn choose_output_file(&self) {
        let window = self.clone();
        glib::spawn_future_local(async move {
            let dialog = gtk::FileDialog::builder()
                .title(gettext("Save Merged PDF"))
                .accept_label(gettext("Merge"))
                .initial_name("Merged.pdf")
                .modal(true)
                .filters(&pdf_filters())
                .build();

            if let Ok(file) = dialog.save_future(Some(&window)).await {
                if let Some(path) = file.path() {
                    window.merge_to(path);
                }
            }
        });
    }

    fn add_files(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }

        let imp = self.imp();
        let paths_to_preview = {
            let previews = imp.merge_previews.borrow();
            paths
                .iter()
                .filter(|path| !previews.contains_key(*path))
                .cloned()
                .collect::<Vec<_>>()
        };
        imp.input_files.borrow_mut().extend(paths);
        imp.last_output.borrow_mut().take();
        self.update_files_view();

        for path in paths_to_preview {
            self.load_merge_preview(path);
        }
    }

    fn load_merge_preview(&self, path: PathBuf) {
        let window = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::preview::render_first_page_preview(path.clone()).await;
            let imp = window.imp();

            if let Ok(Some(preview)) = result {
                if imp.input_files.borrow().contains(&path) {
                    imp.merge_previews.borrow_mut().insert(path, preview);
                    window.update_files_view();
                }
            }
        });
    }

    fn merge_to(&self, output_file: PathBuf) {
        let imp = self.imp();
        let input_files = imp.input_files.borrow().clone();

        imp.is_running.set(true);
        imp.last_output.borrow_mut().take();
        self.update_all_views();

        let window = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::pdf::merge_pdfs(input_files, output_file).await;
            let imp = window.imp();
            imp.is_running.set(false);

            match result {
                Ok(path) => {
                    imp.last_output.borrow_mut().replace(path);
                    window.show_toast(&gettext("Merged PDF saved"));
                }
                Err(error) => {
                    window.show_toast(&error.to_string());
                }
            }

            window.update_all_views();
        });
    }

    pub(super) fn update_files_view(&self) {
        self.update_view_mode();

        let imp = self.imp();
        let files = imp.input_files.borrow();
        let has_files = !files.is_empty();
        let can_merge = files.len() > 1 && !imp.is_running.get();
        let previews = imp.merge_previews.borrow();

        imp.file_list.remove_all();
        imp.merge_file_grid.remove_all();
        for (index, path) in files.iter().enumerate() {
            imp.file_list
                .append(&self.file_row(index, path, files.len(), previews.get(path)));
            imp.merge_file_grid.append(&self.file_tile(
                index,
                path,
                files.len(),
                previews.get(path),
            ));
        }

        imp.empty_status.set_visible(!has_files);
        imp.merge_view_stack.set_visible(has_files);
        imp.add_button.set_visible(has_files);
        imp.clear_button.set_visible(has_files);
        imp.merge_button.set_visible(has_files);
        imp.open_output_button
            .set_visible(imp.last_output.borrow().is_some());

        imp.add_button
            .set_sensitive(has_files && !imp.is_running.get());
        imp.clear_button
            .set_sensitive(has_files && !imp.is_running.get());
        imp.merge_button.set_sensitive(can_merge);
        imp.open_output_button
            .set_sensitive(imp.last_output.borrow().is_some() && !imp.is_running.get());

        let count_text = if imp.is_running.get() {
            gettext("Merging PDFs...")
        } else {
            match files.len() {
                0 => gettext("No files selected"),
                count => ngettext("1 PDF selected", "{} PDFs selected", count as u32)
                    .replace("{}", &count.to_string()),
            }
        };
        imp.file_count_label.set_label(&count_text);
    }

    fn file_row(
        &self,
        index: usize,
        path: &Path,
        count: usize,
        preview: Option<&crate::preview::PagePreview>,
    ) -> adw::ActionRow {
        let row = adw::ActionRow::builder()
            .title(file_title(path))
            .subtitle(file_subtitle(path))
            .activatable(false)
            .build();

        row.add_prefix(&list_preview_prefix(preview));

        let controls_sensitive = !self.imp().is_running.get();
        let up_button = icon_button("go-up-symbolic", &gettext("Move Up"));
        up_button.set_sensitive(controls_sensitive && index > 0);
        let window = self.clone();
        up_button.connect_clicked(move |_| {
            window.move_file(index, index - 1);
        });
        row.add_suffix(&up_button);

        let down_button = icon_button("go-down-symbolic", &gettext("Move Down"));
        down_button.set_sensitive(controls_sensitive && index + 1 < count);
        let window = self.clone();
        down_button.connect_clicked(move |_| {
            window.move_file(index, index + 1);
        });
        row.add_suffix(&down_button);

        let remove_button = icon_button("edit-delete-symbolic", &gettext("Remove"));
        remove_button.set_sensitive(controls_sensitive);
        let window = self.clone();
        remove_button.connect_clicked(move |_| {
            window.remove_file(index);
        });
        row.add_suffix(&remove_button);

        self.add_file_drag_and_drop(&row, index);

        row
    }

    fn file_tile(
        &self,
        index: usize,
        path: &Path,
        count: usize,
        preview: Option<&crate::preview::PagePreview>,
    ) -> gtk::Box {
        let tile = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .width_request(180)
            .build();

        if let Some(preview) = preview {
            let picture = preview_picture(preview);
            picture.set_size_request(160, 220);
            tile.append(&picture);
        } else {
            let placeholder = gtk::Image::from_icon_name("view-paged-symbolic");
            placeholder.set_size_request(160, 220);
            tile.append(&placeholder);
        }

        let label = gtk::Label::builder()
            .label(file_title(path))
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        tile.append(&label);

        let controls = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        let size = gtk::Label::builder()
            .label(file_subtitle(path))
            .xalign(0.0)
            .hexpand(true)
            .build();
        size.add_css_class("dim-label");
        controls.append(&size);

        let controls_sensitive = !self.imp().is_running.get();
        let up_button = icon_button("go-up-symbolic", &gettext("Move Up"));
        up_button.set_sensitive(controls_sensitive && index > 0);
        let window = self.clone();
        up_button.connect_clicked(move |_| {
            window.move_file(index, index - 1);
        });
        controls.append(&up_button);

        let down_button = icon_button("go-down-symbolic", &gettext("Move Down"));
        down_button.set_sensitive(controls_sensitive && index + 1 < count);
        let window = self.clone();
        down_button.connect_clicked(move |_| {
            window.move_file(index, index + 1);
        });
        controls.append(&down_button);

        let remove_button = icon_button("edit-delete-symbolic", &gettext("Remove"));
        remove_button.set_sensitive(controls_sensitive);
        let window = self.clone();
        remove_button.connect_clicked(move |_| {
            window.remove_file(index);
        });
        controls.append(&remove_button);

        tile.append(&controls);
        self.add_file_drag_and_drop(&tile, index);

        tile
    }

    fn move_file(&self, from: usize, to: usize) {
        let imp = self.imp();
        let mut files = imp.input_files.borrow_mut();
        files.swap(from, to);
        imp.last_output.borrow_mut().take();
        drop(files);
        self.update_files_view();
    }

    fn reorder_file(&self, from: usize, to: usize) {
        if from == to {
            return;
        }

        let imp = self.imp();
        let mut files = imp.input_files.borrow_mut();
        if from >= files.len() || to >= files.len() {
            return;
        }

        let file = files.remove(from);
        files.insert(to, file);
        imp.last_output.borrow_mut().take();
        drop(files);

        self.update_files_view();
    }

    fn add_file_drag_and_drop(&self, widget: &impl IsA<gtk::Widget>, index: usize) {
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

            window.reorder_file(from as usize, index);
            true
        });
        widget.add_controller(drop_target);
    }

    fn remove_file(&self, index: usize) {
        let imp = self.imp();
        imp.input_files.borrow_mut().remove(index);
        imp.last_output.borrow_mut().take();
        self.update_files_view();
    }
}
