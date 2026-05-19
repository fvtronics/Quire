use super::ui::{
    icon_button, page_count_label, pdf_filters, rotated_list_preview_prefix,
    rotated_preview_picture,
};
use super::FoliosWindow;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use std::path::PathBuf;

impl FoliosWindow {
    pub(super) fn setup_organize_callbacks(&self) {
        let imp = self.imp();

        let window = self.clone();
        imp.organize_choose_button.connect_clicked(move |_| {
            window.choose_organize_file();
        });

        let window = self.clone();
        imp.organize_empty_choose_button.connect_clicked(move |_| {
            window.choose_organize_file();
        });

        let window = self.clone();
        imp.organize_reset_button.connect_clicked(move |_| {
            window.reset_organize_pdf();
        });

        let window = self.clone();
        imp.organize_save_button.connect_clicked(move |_| {
            window.choose_organize_output_file();
        });

        let window = self.clone();
        imp.organize_open_output_button.connect_clicked(move |_| {
            window.open_last_output();
        });
    }

    fn choose_organize_file(&self) {
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
                    window.load_organize_pdf(path);
                }
            }
        });
    }

    fn choose_organize_output_file(&self) {
        let window = self.clone();
        glib::spawn_future_local(async move {
            let dialog = gtk::FileDialog::builder()
                .title(gettext("Save Organized PDF"))
                .accept_label(gettext("Save"))
                .initial_name("Organized.pdf")
                .modal(true)
                .filters(&pdf_filters())
                .build();

            if let Ok(file) = dialog.save_future(Some(&window)).await {
                if let Some(path) = file.path() {
                    window.organize_to(path);
                }
            }
        });
    }

    fn load_organize_pdf(&self, path: PathBuf) {
        let imp = self.imp();
        imp.is_running.set(true);
        imp.organize_last_output.borrow_mut().take();
        self.update_organize_view();

        let window = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::preview::render_page_previews(path.clone()).await;
            let imp = window.imp();
            imp.is_running.set(false);

            match result {
                Ok(previews) => {
                    let page_count = previews.len();
                    imp.organize_file.borrow_mut().replace(path);
                    imp.organize_page_count.set(page_count);
                    *imp.organize_previews.borrow_mut() = previews;
                    let mut page_order = imp.organize_page_order.borrow_mut();
                    page_order.clear();
                    page_order.extend(1..=page_count as u32);
                    imp.organize_rotations.borrow_mut().clear();
                }
                Err(error) => {
                    window.show_toast(&error.to_string());
                }
            }

            window.update_organize_view();
        });
    }

    fn reset_organize_pdf(&self) {
        let imp = self.imp();
        let page_count = imp.organize_page_count.get();

        if imp.organize_file.borrow().is_none() || page_count == 0 {
            return;
        }

        let mut page_order = imp.organize_page_order.borrow_mut();
        page_order.clear();
        page_order.extend(1..=page_count as u32);
        imp.organize_rotations.borrow_mut().clear();
        imp.organize_last_output.borrow_mut().take();
        drop(page_order);
        self.update_organize_view();
    }

    fn organize_to(&self, output_file: PathBuf) {
        let imp = self.imp();
        let Some(input_file) = imp.organize_file.borrow().clone() else {
            return;
        };
        let rotations = imp.organize_rotations.borrow();
        let page_order = imp
            .organize_page_order
            .borrow()
            .iter()
            .map(|page_number| crate::pdf::PageSelection {
                page_number: *page_number,
                rotation: *rotations.get(page_number).unwrap_or(&0),
            })
            .collect::<Vec<_>>();

        imp.is_running.set(true);
        imp.organize_last_output.borrow_mut().take();
        self.update_all_views();

        let window = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::pdf::organize_pdf(input_file, page_order, output_file).await;
            let imp = window.imp();
            imp.is_running.set(false);

            match result {
                Ok(path) => {
                    imp.organize_last_output.borrow_mut().replace(path);
                    window.show_toast(&gettext("Organized PDF saved"));
                }
                Err(error) => {
                    window.show_toast(&error.to_string());
                }
            }

            window.update_all_views();
        });
    }

    pub(super) fn update_organize_view(&self) {
        self.update_view_mode();

        let imp = self.imp();
        let page_order = imp.organize_page_order.borrow();
        let has_file = imp.organize_file.borrow().is_some();
        let has_pages = !page_order.is_empty();
        let previews = imp.organize_previews.borrow();
        let rotations = imp.organize_rotations.borrow();

        imp.organize_page_list.remove_all();
        imp.organize_page_grid.remove_all();
        for (index, page_number) in page_order.iter().enumerate() {
            let preview = previews
                .iter()
                .find(|preview| preview.page_number == *page_number);
            let rotation = *rotations.get(page_number).unwrap_or(&0);
            imp.organize_page_list.append(&self.page_row(
                index,
                *page_number,
                page_order.len(),
                preview,
                rotation,
            ));
            if let Some(preview) = preview {
                imp.organize_page_grid.append(&self.organize_page_tile(
                    preview,
                    index,
                    page_order.len(),
                    rotation,
                ));
            }
        }

        imp.organize_empty_status.set_visible(!has_file);
        imp.organize_view_stack.set_visible(has_file);
        imp.organize_choose_button.set_visible(has_file);
        imp.organize_reset_button.set_visible(has_file);
        imp.organize_save_button.set_visible(has_file);
        imp.organize_open_output_button
            .set_visible(imp.organize_last_output.borrow().is_some());

        imp.organize_choose_button
            .set_sensitive(!imp.is_running.get());
        imp.organize_empty_choose_button
            .set_sensitive(!imp.is_running.get());
        imp.organize_reset_button
            .set_sensitive(has_file && !imp.is_running.get());
        imp.organize_save_button
            .set_sensitive(has_pages && !imp.is_running.get());
        imp.organize_open_output_button
            .set_sensitive(imp.organize_last_output.borrow().is_some() && !imp.is_running.get());

        let detail = if imp.is_running.get() {
            gettext("Organizing pages...")
        } else if has_file {
            page_count_label(page_order.len())
        } else {
            gettext("No PDF selected")
        };
        imp.organize_detail_label.set_label(&detail);
    }

    fn page_row(
        &self,
        index: usize,
        page_number: u32,
        count: usize,
        preview: Option<&crate::preview::PagePreview>,
        rotation: i64,
    ) -> adw::ActionRow {
        let row = adw::ActionRow::builder()
            .title(format!("{} {page_number}", gettext("Page")))
            .subtitle(format!(
                "{} {} {} {count}",
                gettext("Position"),
                index + 1,
                gettext("of")
            ))
            .activatable(false)
            .build();

        row.add_prefix(&rotated_list_preview_prefix(preview, rotation));

        let controls_sensitive = !self.imp().is_running.get();
        let up_button = icon_button("go-up-symbolic", &gettext("Move Up"));
        up_button.set_sensitive(controls_sensitive && index > 0);
        let window = self.clone();
        up_button.connect_clicked(move |_| {
            window.move_page(index, index - 1);
        });
        row.add_suffix(&up_button);

        let down_button = icon_button("go-down-symbolic", &gettext("Move Down"));
        down_button.set_sensitive(controls_sensitive && index + 1 < count);
        let window = self.clone();
        down_button.connect_clicked(move |_| {
            window.move_page(index, index + 1);
        });
        row.add_suffix(&down_button);

        let rotate_button =
            icon_button("object-rotate-right-symbolic", &gettext("Rotate Clockwise"));
        rotate_button.set_sensitive(controls_sensitive);
        let window = self.clone();
        rotate_button.connect_clicked(move |_| {
            window.rotate_page(page_number);
        });
        row.add_suffix(&rotate_button);

        let remove_button = icon_button("edit-delete-symbolic", &gettext("Remove"));
        remove_button.set_sensitive(controls_sensitive && count > 1);
        let window = self.clone();
        remove_button.connect_clicked(move |_| {
            window.remove_page(index);
        });
        row.add_suffix(&remove_button);

        self.add_page_drag_and_drop(&row, page_number);

        row
    }

    fn organize_page_tile(
        &self,
        preview: &crate::preview::PagePreview,
        index: usize,
        count: usize,
        rotation: i64,
    ) -> gtk::Box {
        let tile = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .width_request(180)
            .build();

        let picture = rotated_preview_picture(preview, rotation);
        picture.set_size_request(160, 220);
        tile.append(&picture);

        let label = gtk::Label::builder()
            .label(format!("{} {}", gettext("Page"), preview.page_number))
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        tile.append(&label);

        let controls = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();
        let position = gtk::Label::builder()
            .label(format!("{}/{}", index + 1, count))
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        position.add_css_class("dim-label");
        controls.append(&position);

        let controls_sensitive = !self.imp().is_running.get();
        let up_button = icon_button("go-up-symbolic", &gettext("Move Up"));
        up_button.set_sensitive(controls_sensitive && index > 0);
        let window = self.clone();
        up_button.connect_clicked(move |_| {
            window.move_page(index, index - 1);
        });
        controls.append(&up_button);

        let down_button = icon_button("go-down-symbolic", &gettext("Move Down"));
        down_button.set_sensitive(controls_sensitive && index + 1 < count);
        let window = self.clone();
        down_button.connect_clicked(move |_| {
            window.move_page(index, index + 1);
        });
        controls.append(&down_button);

        let rotate_button =
            icon_button("object-rotate-right-symbolic", &gettext("Rotate Clockwise"));
        rotate_button.set_sensitive(controls_sensitive);
        let page_number = preview.page_number;
        let window = self.clone();
        rotate_button.connect_clicked(move |_| {
            window.rotate_page(page_number);
        });
        controls.append(&rotate_button);

        let remove_button = icon_button("edit-delete-symbolic", &gettext("Remove"));
        remove_button.set_sensitive(controls_sensitive && count > 1);
        let window = self.clone();
        remove_button.connect_clicked(move |_| {
            window.remove_page(index);
        });
        controls.append(&remove_button);

        tile.append(&controls);

        self.add_page_drag_and_drop(&tile, preview.page_number);

        tile
    }

    fn move_page(&self, from: usize, to: usize) {
        let imp = self.imp();
        let mut pages = imp.organize_page_order.borrow_mut();
        pages.swap(from, to);
        imp.organize_last_output.borrow_mut().take();
        drop(pages);
        self.update_organize_view();
    }

    fn rotate_page(&self, page_number: u32) {
        let imp = self.imp();
        let mut rotations = imp.organize_rotations.borrow_mut();
        let rotation = (rotations.get(&page_number).copied().unwrap_or(0) + 90).rem_euclid(360);
        if rotation == 0 {
            rotations.remove(&page_number);
        } else {
            rotations.insert(page_number, rotation);
        }
        imp.organize_last_output.borrow_mut().take();
        drop(rotations);

        self.update_organize_view();
    }

    fn reorder_page(&self, dragged_page: u32, target_page: u32) {
        if dragged_page == target_page {
            return;
        }

        let imp = self.imp();
        let mut pages = imp.organize_page_order.borrow_mut();
        let Some(from) = pages.iter().position(|page| *page == dragged_page) else {
            return;
        };
        let Some(to) = pages.iter().position(|page| *page == target_page) else {
            return;
        };

        let page = pages.remove(from);
        pages.insert(to, page);
        imp.organize_last_output.borrow_mut().take();
        drop(pages);

        self.update_organize_view();
    }

    fn add_page_drag_and_drop(&self, widget: &impl IsA<gtk::Widget>, page_number: u32) {
        let drag_source = gtk::DragSource::builder()
            .actions(gtk::gdk::DragAction::MOVE)
            .build();
        drag_source.connect_prepare(move |_, _, _| {
            Some(gtk::gdk::ContentProvider::for_value(
                &page_number.to_value(),
            ))
        });
        widget.add_controller(drag_source);

        let drop_target = gtk::DropTarget::new(u32::static_type(), gtk::gdk::DragAction::MOVE);
        let window = self.clone();
        drop_target.connect_drop(move |_, value, _, _| {
            let Ok(dragged_page) = value.get::<u32>() else {
                return false;
            };

            window.reorder_page(dragged_page, page_number);
            true
        });
        widget.add_controller(drop_target);
    }

    fn remove_page(&self, index: usize) {
        let imp = self.imp();
        if imp.organize_page_order.borrow().len() <= 1 {
            return;
        }

        let page_number = imp.organize_page_order.borrow_mut().remove(index);
        imp.organize_rotations.borrow_mut().remove(&page_number);
        imp.organize_last_output.borrow_mut().take();

        self.update_organize_view();
    }
}
