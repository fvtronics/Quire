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
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum PdfTool {
    #[default]
    Merge,
    Organize,
    Extract,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ViewMode {
    #[default]
    List,
    Grid,
}

const LIST_VIEW_NAME: &str = "list";
const GRID_VIEW_NAME: &str = "grid";

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/fvtronics/folios/window.ui")]
    pub struct FoliosWindow {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub sidebar_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub list_view_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub grid_view_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub merge_tool_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub organize_tool_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub extract_tool_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub merge_workspace: TemplateChild<gtk::Box>,
        #[template_child]
        pub organize_workspace: TemplateChild<gtk::Box>,
        #[template_child]
        pub extract_workspace: TemplateChild<gtk::Box>,

        #[template_child]
        pub add_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub empty_add_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub file_count_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub empty_status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub merge_view_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub file_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub merge_file_grid: TemplateChild<gtk::FlowBox>,
        #[template_child]
        pub clear_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub merge_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub open_output_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub organize_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub organize_empty_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub organize_detail_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub organize_empty_status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub organize_view_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub organize_page_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub organize_page_grid: TemplateChild<gtk::FlowBox>,
        #[template_child]
        pub organize_reset_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub organize_save_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub organize_open_output_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub extract_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub extract_empty_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub extract_detail_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub extract_empty_status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub extract_content: TemplateChild<gtk::Box>,
        #[template_child]
        pub extract_file_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub extract_ranges_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub extract_view_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub extract_page_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub extract_page_grid: TemplateChild<gtk::FlowBox>,
        #[template_child]
        pub extract_clear_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub extract_save_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub extract_open_output_button: TemplateChild<gtk::Button>,

        pub(super) active_tool: Cell<PdfTool>,
        pub(super) view_mode: Cell<ViewMode>,
        pub input_files: RefCell<Vec<PathBuf>>,
        pub merge_previews: RefCell<BTreeMap<PathBuf, crate::preview::PagePreview>>,
        pub last_output: RefCell<Option<PathBuf>>,
        pub organize_file: RefCell<Option<PathBuf>>,
        pub organize_page_count: Cell<usize>,
        pub organize_previews: RefCell<Vec<crate::preview::PagePreview>>,
        pub organize_page_order: RefCell<Vec<u32>>,
        pub organize_last_output: RefCell<Option<PathBuf>>,
        pub extract_file: RefCell<Option<PathBuf>>,
        pub extract_page_count: Cell<usize>,
        pub extract_previews: RefCell<Vec<crate::preview::PagePreview>>,
        pub extract_selected_pages: RefCell<Vec<u32>>,
        pub extract_last_output: RefCell<Option<PathBuf>>,
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
            obj.switch_tool(PdfTool::Merge);
            obj.update_all_views();
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
        imp.list_view_button.connect_clicked(move |_| {
            window.set_view_mode(ViewMode::List);
        });

        let window = self.clone();
        imp.grid_view_button.connect_clicked(move |_| {
            window.set_view_mode(ViewMode::Grid);
        });

        let window = self.clone();
        imp.merge_tool_row.connect_activated(move |_| {
            window.switch_tool(PdfTool::Merge);
        });

        let window = self.clone();
        imp.organize_tool_row.connect_activated(move |_| {
            window.switch_tool(PdfTool::Organize);
        });

        let window = self.clone();
        imp.extract_tool_row.connect_activated(move |_| {
            window.switch_tool(PdfTool::Extract);
        });

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

        let window = self.clone();
        imp.extract_choose_button.connect_clicked(move |_| {
            window.choose_extract_file();
        });

        let window = self.clone();
        imp.extract_empty_choose_button.connect_clicked(move |_| {
            window.choose_extract_file();
        });

        let window = self.clone();
        imp.extract_clear_button.connect_clicked(move |_| {
            window.clear_extract_pdf();
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

    fn switch_tool(&self, tool: PdfTool) {
        let imp = self.imp();
        imp.active_tool.set(tool);
        imp.merge_workspace.set_visible(tool == PdfTool::Merge);
        imp.organize_workspace
            .set_visible(tool == PdfTool::Organize);
        imp.extract_workspace.set_visible(tool == PdfTool::Extract);

        let selected_row: &gtk::ListBoxRow = match tool {
            PdfTool::Merge => imp.merge_tool_row.upcast_ref(),
            PdfTool::Organize => imp.organize_tool_row.upcast_ref(),
            PdfTool::Extract => imp.extract_tool_row.upcast_ref(),
        };
        imp.sidebar_list.select_row(Some(selected_row));
        self.update_view_mode();
    }

    fn set_view_mode(&self, view_mode: ViewMode) {
        self.imp().view_mode.set(view_mode);
        self.update_view_mode();
    }

    fn update_view_mode(&self) {
        let imp = self.imp();
        let view_mode = imp.view_mode.get();
        let view_name = match view_mode {
            ViewMode::List => LIST_VIEW_NAME,
            ViewMode::Grid => GRID_VIEW_NAME,
        };

        imp.list_view_button.set_active(view_mode == ViewMode::List);
        imp.grid_view_button.set_active(view_mode == ViewMode::Grid);
        imp.merge_view_stack.set_visible_child_name(view_name);
        imp.organize_view_stack.set_visible_child_name(view_name);
        imp.extract_view_stack.set_visible_child_name(view_name);
    }

    fn update_all_views(&self) {
        self.update_view_mode();
        self.update_files_view();
        self.update_organize_view();
        self.update_extract_view();
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
            match crate::pdf::parse_page_ranges(
                imp.extract_ranges_entry.text().as_str(),
                imp.extract_page_count.get(),
            ) {
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
                }
                Err(error) => {
                    window.show_toast(&error.to_string());
                }
            }

            window.update_organize_view();
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

    fn clear_organize_pdf(&self) {
        let imp = self.imp();
        imp.organize_file.borrow_mut().take();
        imp.organize_page_count.set(0);
        imp.organize_previews.borrow_mut().clear();
        imp.organize_page_order.borrow_mut().clear();
        imp.organize_last_output.borrow_mut().take();
        self.update_organize_view();
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
        imp.organize_last_output.borrow_mut().take();
        drop(page_order);
        self.update_organize_view();
    }

    fn clear_extract_pdf(&self) {
        let imp = self.imp();
        imp.extract_file.borrow_mut().take();
        imp.extract_page_count.set(0);
        imp.extract_previews.borrow_mut().clear();
        imp.extract_selected_pages.borrow_mut().clear();
        imp.extract_ranges_entry.set_text("");
        imp.extract_last_output.borrow_mut().take();
        self.update_extract_view();
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

    fn organize_to(&self, output_file: PathBuf) {
        let imp = self.imp();
        let Some(input_file) = imp.organize_file.borrow().clone() else {
            return;
        };
        let page_order = imp.organize_page_order.borrow().clone();

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

    fn update_files_view(&self) {
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
                1 => gettext("1 PDF selected"),
                count => format!("{count} PDFs selected"),
            }
        };
        imp.file_count_label.set_label(&count_text);
    }

    fn update_organize_view(&self) {
        let imp = self.imp();
        let page_order = imp.organize_page_order.borrow();
        let has_file = imp.organize_file.borrow().is_some();
        let has_pages = !page_order.is_empty();
        let previews = imp.organize_previews.borrow();

        imp.organize_page_list.remove_all();
        imp.organize_page_grid.remove_all();
        for (index, page_number) in page_order.iter().enumerate() {
            let preview = previews
                .iter()
                .find(|preview| preview.page_number == *page_number);
            imp.organize_page_list.append(&self.page_row(
                index,
                *page_number,
                page_order.len(),
                preview,
            ));
            if let Some(preview) = preview {
                imp.organize_page_grid.append(&self.organize_page_tile(
                    preview,
                    index,
                    page_order.len(),
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
            gettext("Working...")
        } else if has_file {
            page_count_label(page_order.len())
        } else {
            gettext("No PDF selected")
        };
        imp.organize_detail_label.set_label(&detail);
    }

    fn update_extract_view(&self) {
        let imp = self.imp();
        let has_file = imp.extract_file.borrow().is_some();
        let has_ranges = !imp.extract_ranges_entry.text().trim().is_empty();
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
        imp.extract_clear_button.set_visible(has_file);
        imp.extract_save_button.set_visible(has_file);
        imp.extract_open_output_button
            .set_visible(imp.extract_last_output.borrow().is_some());

        imp.extract_choose_button
            .set_sensitive(!imp.is_running.get());
        imp.extract_empty_choose_button
            .set_sensitive(!imp.is_running.get());
        imp.extract_clear_button
            .set_sensitive(has_file && !imp.is_running.get());
        imp.extract_save_button
            .set_sensitive(has_file && (has_ranges || has_selected_pages) && !imp.is_running.get());
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

    fn file_row(
        &self,
        index: usize,
        path: &Path,
        count: usize,
        preview: Option<&crate::preview::PagePreview>,
    ) -> adw::ActionRow {
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("PDF");
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle(file_subtitle(path))
            .activatable(false)
            .build();

        if let Some(preview) = preview {
            let picture = preview_picture(preview);
            picture.set_size_request(48, 68);
            row.add_prefix(&picture);
        } else {
            let icon = gtk::Image::from_icon_name("view-paged-symbolic");
            row.add_prefix(&icon);
        }

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

        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("PDF");
        let label = gtk::Label::builder()
            .label(title)
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

    fn page_row(
        &self,
        index: usize,
        page_number: u32,
        count: usize,
        preview: Option<&crate::preview::PagePreview>,
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

        if let Some(preview) = preview {
            let picture = preview_picture(preview);
            picture.set_size_request(48, 68);
            row.add_prefix(&picture);
        } else {
            let icon = gtk::Image::from_icon_name("view-paged-symbolic");
            row.add_prefix(&icon);
        }

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

        let remove_button = icon_button("edit-delete-symbolic", &gettext("Remove"));
        remove_button.set_sensitive(controls_sensitive);
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
    ) -> gtk::Box {
        let tile = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .width_request(180)
            .build();

        let picture = preview_picture(preview);
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

        let remove_button = icon_button("edit-delete-symbolic", &gettext("Remove"));
        remove_button.set_sensitive(controls_sensitive);
        let window = self.clone();
        remove_button.connect_clicked(move |_| {
            window.remove_page(index);
        });
        controls.append(&remove_button);

        tile.append(&controls);

        self.add_page_drag_and_drop(&tile, preview.page_number);

        tile
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

    fn move_page(&self, from: usize, to: usize) {
        let imp = self.imp();
        let mut pages = imp.organize_page_order.borrow_mut();
        pages.swap(from, to);
        imp.organize_last_output.borrow_mut().take();
        drop(pages);
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
        imp.organize_page_order.borrow_mut().remove(index);
        imp.organize_last_output.borrow_mut().take();

        if imp.organize_page_order.borrow().is_empty() {
            self.clear_organize_pdf();
            return;
        }

        self.update_organize_view();
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

    fn open_last_output(&self) {
        let imp = self.imp();
        let path = match imp.active_tool.get() {
            PdfTool::Merge => imp.last_output.borrow().clone(),
            PdfTool::Organize => imp.organize_last_output.borrow().clone(),
            PdfTool::Extract => imp.extract_last_output.borrow().clone(),
        };
        let Some(path) = path else {
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

fn preview_picture(preview: &crate::preview::PagePreview) -> gtk::Picture {
    let picture = match gtk::gdk_pixbuf::Pixbuf::from_read(Cursor::new(preview.png_data.clone())) {
        Ok(pixbuf) => {
            let texture = gtk::gdk::Texture::for_pixbuf(&pixbuf);
            gtk::Picture::for_paintable(&texture)
        }
        Err(_) => gtk::Picture::new(),
    };
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture
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

fn normalize_pages(mut pages: Vec<u32>) -> Vec<u32> {
    pages.sort_unstable();
    pages.dedup();
    pages
}

fn format_page_ranges(pages: &[u32]) -> String {
    let Some((&first, rest)) = pages.split_first() else {
        return String::new();
    };

    let mut parts = Vec::new();
    let mut start = first;
    let mut end = first;

    for page in rest {
        if *page == end + 1 {
            end = *page;
        } else {
            parts.push(format_page_range(start, end));
            start = *page;
            end = *page;
        }
    }

    parts.push(format_page_range(start, end));
    parts.join(",")
}

fn format_page_range(start: u32, end: u32) -> String {
    if start == end {
        start.to_string()
    } else {
        format!("{start}-{end}")
    }
}

fn page_count_label(count: usize) -> String {
    match count {
        1 => gettext("1 page"),
        count => format!("{count} pages"),
    }
}
