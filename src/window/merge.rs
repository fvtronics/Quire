use super::ui::{
    dim_tile_label, file_subtitle, file_title, icon_button, open_pdf_files, preview_tile,
    rotated_list_preview_prefix, save_pdf_file, tile_controls, tile_label, tile_preview_widget,
};
use super::FoliosWindow;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::glib;
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
            imp.merge.files.borrow_mut().clear();
            imp.merge.rotations.borrow_mut().clear();
            imp.merge.previews.borrow_mut().clear();
            imp.merge.last_output.borrow_mut().take();
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
            let paths = open_pdf_files(&window, &gettext("Add PDFs"), &gettext("Add")).await;
            if !paths.is_empty() {
                window.add_files(paths);
            }
        });
    }

    fn choose_output_file(&self) {
        let window = self.clone();
        glib::spawn_future_local(async move {
            if let Some(path) = save_pdf_file(
                &window,
                &gettext("Save Merged PDF"),
                &gettext("Merge"),
                "Merged.pdf",
            )
            .await
            {
                window.merge_to(path);
            }
        });
    }

    fn add_files(&self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }

        let imp = self.imp();
        let paths_to_preview = {
            let previews = imp.merge.previews.borrow();
            paths
                .iter()
                .filter(|path| !previews.contains_key(*path))
                .cloned()
                .collect::<Vec<_>>()
        };
        imp.merge.last_output.borrow_mut().take();

        if paths_to_preview.is_empty() {
            imp.merge.files.borrow_mut().extend(paths);
            self.update_files_view();
            return;
        }

        imp.merge.is_loading.set(true);
        self.update_files_view();
        self.load_merge_previews(paths, paths_to_preview);
    }

    fn load_merge_previews(&self, paths: Vec<PathBuf>, paths_to_preview: Vec<PathBuf>) {
        let window = self.clone();
        glib::spawn_future_local(async move {
            let mut loaded_paths = Vec::new();
            let mut loaded_previews = Vec::new();

            for path in &paths_to_preview {
                match crate::preview::render_first_page_preview(path.clone()).await {
                    Ok(preview) => {
                        loaded_paths.push(path.clone());
                        if let Some(preview) = preview {
                            loaded_previews.push((path.clone(), preview));
                        }
                    }
                    Err(error) => {
                        window.show_toast(&error.to_string());
                    }
                }
            }

            let files_to_add = paths
                .into_iter()
                .filter(|path| !paths_to_preview.contains(path) || loaded_paths.contains(path))
                .collect::<Vec<_>>();
            let imp = window.imp();
            imp.merge.is_loading.set(false);

            if !loaded_previews.is_empty() {
                imp.merge.previews.borrow_mut().extend(loaded_previews);
            }
            if !files_to_add.is_empty() {
                imp.merge.files.borrow_mut().extend(files_to_add);
            }

            window.update_files_view();
        });
    }

    fn merge_to(&self, output_file: PathBuf) {
        let imp = self.imp();
        let rotations = imp.merge.rotations.borrow();
        let input_files = imp
            .merge
            .files
            .borrow()
            .iter()
            .map(|path| crate::pdf::PdfInput {
                path: path.clone(),
                rotation: *rotations.get(path).unwrap_or(&0),
            })
            .collect::<Vec<_>>();

        imp.is_running.set(true);
        imp.merge.last_output.borrow_mut().take();
        self.update_all_views();

        let window = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::pdf::merge_pdfs(input_files, output_file).await;
            let imp = window.imp();
            imp.is_running.set(false);

            match result {
                Ok(path) => {
                    imp.merge.last_output.borrow_mut().replace(path);
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
        let files = imp.merge.files.borrow();
        let has_files = !files.is_empty();
        let is_busy = imp.is_running.get() || imp.merge.is_loading.get();
        let can_merge = files.len() > 1 && !is_busy;
        let previews = imp.merge.previews.borrow();
        let rotations = imp.merge.rotations.borrow();

        imp.file_list.remove_all();
        imp.merge_file_grid.remove_all();
        for (index, path) in files.iter().enumerate() {
            let rotation = *rotations.get(path).unwrap_or(&0);
            imp.file_list.append(&self.file_row(
                index,
                path,
                files.len(),
                previews.get(path),
                rotation,
            ));
            imp.merge_file_grid.append(&self.file_tile(
                index,
                path,
                files.len(),
                previews.get(path),
                rotation,
            ));
        }

        imp.empty_status.set_visible(!has_files);
        imp.merge_view_stack.set_visible(has_files);
        imp.add_button.set_visible(has_files);
        imp.clear_button.set_visible(has_files);
        imp.merge_button.set_visible(has_files);
        imp.open_output_button
            .set_visible(imp.merge.last_output.borrow().is_some());

        imp.add_button.set_sensitive(has_files && !is_busy);
        imp.empty_add_button.set_sensitive(!is_busy);
        imp.clear_button.set_sensitive(has_files && !is_busy);
        imp.merge_button.set_sensitive(can_merge);
        imp.open_output_button
            .set_sensitive(imp.merge.last_output.borrow().is_some() && !is_busy);

        let count_text = if imp.is_running.get() {
            gettext("Merging PDFs...")
        } else if imp.merge.is_loading.get() {
            gettext("Loading PDFs...")
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
        rotation: i64,
    ) -> adw::ActionRow {
        let row = adw::ActionRow::builder()
            .title(file_title(path))
            .subtitle(file_subtitle(path))
            .activatable(false)
            .build();

        row.add_prefix(&rotated_list_preview_prefix(preview, rotation));

        let imp = self.imp();
        let controls_sensitive = !imp.is_running.get() && !imp.merge.is_loading.get();
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

        let rotate_button =
            icon_button("object-rotate-right-symbolic", &gettext("Rotate Clockwise"));
        rotate_button.set_sensitive(controls_sensitive);
        let window = self.clone();
        rotate_button.connect_clicked(move |_| {
            window.rotate_file(index);
        });
        row.add_suffix(&rotate_button);

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
        rotation: i64,
    ) -> gtk::Box {
        let tile = preview_tile();
        tile.append(&tile_preview_widget(preview, rotation));
        tile.append(&tile_label(file_title(path)));

        let controls = tile_controls();
        let size = dim_tile_label(file_subtitle(path));
        controls.append(&size);

        let imp = self.imp();
        let controls_sensitive = !imp.is_running.get() && !imp.merge.is_loading.get();
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

        let rotate_button =
            icon_button("object-rotate-right-symbolic", &gettext("Rotate Clockwise"));
        rotate_button.set_sensitive(controls_sensitive);
        let window = self.clone();
        rotate_button.connect_clicked(move |_| {
            window.rotate_file(index);
        });
        controls.append(&rotate_button);

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
        let mut files = imp.merge.files.borrow_mut();
        files.swap(from, to);
        imp.merge.last_output.borrow_mut().take();
        drop(files);
        self.update_files_view();
    }

    fn rotate_file(&self, index: usize) {
        let imp = self.imp();
        let Some(path) = imp.merge.files.borrow().get(index).cloned() else {
            return;
        };

        let mut rotations = imp.merge.rotations.borrow_mut();
        let rotation = (rotations.get(&path).copied().unwrap_or(0) + 90).rem_euclid(360);
        if rotation == 0 {
            rotations.remove(&path);
        } else {
            rotations.insert(path, rotation);
        }
        imp.merge.last_output.borrow_mut().take();
        drop(rotations);

        self.update_files_view();
    }

    fn reorder_file(&self, from: usize, to: usize) {
        if from == to {
            return;
        }

        let imp = self.imp();
        let mut files = imp.merge.files.borrow_mut();
        if from >= files.len() || to >= files.len() {
            return;
        }

        let file = files.remove(from);
        files.insert(to, file);
        imp.merge.last_output.borrow_mut().take();
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
        let path = imp.merge.files.borrow_mut().remove(index);
        if !imp.merge.files.borrow().contains(&path) {
            imp.merge.rotations.borrow_mut().remove(&path);
        }
        imp.merge.last_output.borrow_mut().take();
        self.update_files_view();
    }
}
