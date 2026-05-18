/* window.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/fvtronics/folios/window.ui")]
    pub struct FoliosWindow {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub add_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub empty_add_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub file_count_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub empty_status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub file_scroller: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub file_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub clear_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub merge_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub open_output_button: TemplateChild<gtk::Button>,

        pub input_files: RefCell<Vec<PathBuf>>,
        pub last_output: RefCell<Option<PathBuf>>,
        pub is_running: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FoliosWindow {
        const NAME: &'static str = "FoliosWindow";
        type Type = super::FoliosWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for FoliosWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_callbacks();
            obj.update_files_view();
        }
    }
    impl WidgetImpl for FoliosWindow {}
    impl WindowImpl for FoliosWindow {}
    impl ApplicationWindowImpl for FoliosWindow {}
    impl AdwApplicationWindowImpl for FoliosWindow {}
}

glib::wrapper! {
    pub struct FoliosWindow(ObjectSubclass<imp::FoliosWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl FoliosWindow {
    pub fn new<P: IsA<gtk::Application>>(application: &P) -> Self {
        glib::Object::builder()
            .property("application", application)
            .build()
    }

    fn setup_callbacks(&self) {
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
        imp.input_files.borrow_mut().extend(paths);
        imp.last_output.borrow_mut().take();
        self.update_files_view();
    }

    fn merge_to(&self, output_file: PathBuf) {
        let imp = self.imp();
        let input_files = imp.input_files.borrow().clone();

        imp.is_running.set(true);
        imp.last_output.borrow_mut().take();
        self.update_files_view();

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

            window.update_files_view();
        });
    }

    fn update_files_view(&self) {
        let imp = self.imp();
        let files = imp.input_files.borrow();
        let has_files = !files.is_empty();
        let can_merge = files.len() > 1 && !imp.is_running.get();

        imp.file_list.remove_all();
        for (index, path) in files.iter().enumerate() {
            imp.file_list
                .append(&self.file_row(index, path, files.len()));
        }

        imp.empty_status.set_visible(!has_files);
        imp.file_scroller.set_visible(has_files);
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
                1 => gettext("1 PDF selected"),
                count => format!("{count} PDFs selected"),
            }
        };
        imp.file_count_label.set_label(&count_text);
    }

    fn file_row(&self, index: usize, path: &Path, count: usize) -> adw::ActionRow {
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

        let up_button = icon_button("go-up-symbolic", &gettext("Move Up"));
        up_button.set_sensitive(index > 0);
        let window = self.clone();
        up_button.connect_clicked(move |_| {
            window.move_file(index, index - 1);
        });
        row.add_suffix(&up_button);

        let down_button = icon_button("go-down-symbolic", &gettext("Move Down"));
        down_button.set_sensitive(index + 1 < count);
        let window = self.clone();
        down_button.connect_clicked(move |_| {
            window.move_file(index, index + 1);
        });
        row.add_suffix(&down_button);

        let remove_button = icon_button("edit-delete-symbolic", &gettext("Remove"));
        let window = self.clone();
        remove_button.connect_clicked(move |_| {
            window.remove_file(index);
        });
        row.add_suffix(&remove_button);

        row
    }

    fn move_file(&self, from: usize, to: usize) {
        let imp = self.imp();
        let mut files = imp.input_files.borrow_mut();
        files.swap(from, to);
        imp.last_output.borrow_mut().take();
        drop(files);
        self.update_files_view();
    }

    fn remove_file(&self, index: usize) {
        let imp = self.imp();
        imp.input_files.borrow_mut().remove(index);
        imp.last_output.borrow_mut().take();
        self.update_files_view();
    }

    fn open_last_output(&self) {
        let imp = self.imp();
        let Some(path) = imp.last_output.borrow().clone() else {
            return;
        };

        let file = gio::File::for_path(path);
        if let Err(error) = gio::AppInfo::launch_default_for_uri(
            file.uri().as_str(),
            None::<&gio::AppLaunchContext>,
        ) {
            self.show_toast(&error.to_string());
        }
    }

    fn show_toast(&self, message: &str) {
        let imp = self.imp();
        imp.toast_overlay.add_toast(adw::Toast::new(message));
    }
}

fn pdf_filters() -> gio::ListStore {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&gettext("PDF Documents")));
    filter.add_mime_type("application/pdf");
    filter.add_pattern("*.pdf");

    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    filters
}

fn icon_button(icon_name: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon_name)
        .tooltip_text(tooltip)
        .valign(gtk::Align::Center)
        .build();
    button.add_css_class("flat");
    button
}

fn file_subtitle(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(metadata) => format_size(metadata.len()),
        Err(_) => gettext("Size unavailable"),
    }
}

fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;

    let bytes = bytes as f64;
    if bytes >= MB {
        format!("{:.1} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes / KB)
    } else {
        format!("{bytes:.0} B")
    }
}
