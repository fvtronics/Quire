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
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::PathBuf;

mod compress;
mod extract;
mod merge;
mod organize;
mod split;
mod ui;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum PdfTool {
    #[default]
    Merge,
    Compress,
    Organize,
    Extract,
    Split,
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
        pub view_mode_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub list_view_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub grid_view_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub merge_tool_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub compress_tool_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub organize_tool_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub extract_tool_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub split_tool_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub merge_workspace: TemplateChild<gtk::Box>,
        #[template_child]
        pub compress_workspace: TemplateChild<gtk::Box>,
        #[template_child]
        pub organize_workspace: TemplateChild<gtk::Box>,
        #[template_child]
        pub extract_workspace: TemplateChild<gtk::Box>,
        #[template_child]
        pub split_workspace: TemplateChild<gtk::Box>,

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
        pub compress_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub compress_empty_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub compress_detail_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub compress_empty_status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub compress_content: TemplateChild<gtk::Box>,
        #[template_child]
        pub compress_preview_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub compress_file_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub compress_save_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub compress_open_output_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub compress_prune_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub compress_empty_streams_row: TemplateChild<adw::SwitchRow>,

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
        pub extract_save_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub extract_open_output_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub split_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub split_empty_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub split_detail_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub split_empty_status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub split_content: TemplateChild<gtk::Box>,
        #[template_child]
        pub split_preview_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub split_file_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub split_after_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub split_specific_pages_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub split_pages_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub split_prefix_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub split_save_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub split_open_output_button: TemplateChild<gtk::Button>,

        pub(super) active_tool: Cell<PdfTool>,
        pub(super) view_mode: Cell<ViewMode>,
        pub input_files: RefCell<Vec<PathBuf>>,
        pub merge_rotations: RefCell<BTreeMap<PathBuf, i64>>,
        pub merge_previews: RefCell<BTreeMap<PathBuf, crate::preview::PagePreview>>,
        pub last_output: RefCell<Option<PathBuf>>,
        pub compress_file: RefCell<Option<PathBuf>>,
        pub compress_preview: RefCell<Option<crate::preview::PagePreview>>,
        pub compress_last_output: RefCell<Option<PathBuf>>,
        pub organize_file: RefCell<Option<PathBuf>>,
        pub organize_page_count: Cell<usize>,
        pub organize_previews: RefCell<Vec<crate::preview::PagePreview>>,
        pub organize_page_order: RefCell<Vec<u32>>,
        pub organize_rotations: RefCell<BTreeMap<u32, i64>>,
        pub organize_last_output: RefCell<Option<PathBuf>>,
        pub extract_file: RefCell<Option<PathBuf>>,
        pub extract_page_count: Cell<usize>,
        pub extract_previews: RefCell<Vec<crate::preview::PagePreview>>,
        pub extract_selected_pages: RefCell<Vec<u32>>,
        pub extract_rotations: RefCell<BTreeMap<u32, i64>>,
        pub extract_last_output: RefCell<Option<PathBuf>>,
        pub split_file: RefCell<Option<PathBuf>>,
        pub split_page_count: Cell<usize>,
        pub split_preview: RefCell<Option<crate::preview::PagePreview>>,
        pub split_last_output: RefCell<Option<PathBuf>>,
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
        imp.compress_tool_row.connect_activated(move |_| {
            window.switch_tool(PdfTool::Compress);
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
        imp.split_tool_row.connect_activated(move |_| {
            window.switch_tool(PdfTool::Split);
        });

        self.setup_merge_callbacks();
        self.setup_compress_callbacks();
        self.setup_organize_callbacks();
        self.setup_extract_callbacks();
        self.setup_split_callbacks();
    }

    fn switch_tool(&self, tool: PdfTool) {
        let imp = self.imp();
        imp.active_tool.set(tool);
        imp.merge_workspace.set_visible(tool == PdfTool::Merge);
        imp.compress_workspace
            .set_visible(tool == PdfTool::Compress);
        imp.organize_workspace
            .set_visible(tool == PdfTool::Organize);
        imp.extract_workspace.set_visible(tool == PdfTool::Extract);
        imp.split_workspace.set_visible(tool == PdfTool::Split);

        let selected_row: &gtk::ListBoxRow = match tool {
            PdfTool::Merge => imp.merge_tool_row.upcast_ref(),
            PdfTool::Compress => imp.compress_tool_row.upcast_ref(),
            PdfTool::Organize => imp.organize_tool_row.upcast_ref(),
            PdfTool::Extract => imp.extract_tool_row.upcast_ref(),
            PdfTool::Split => imp.split_tool_row.upcast_ref(),
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

        imp.view_mode_box
            .set_visible(self.active_tool_has_view_mode_content());
        imp.list_view_button.set_active(view_mode == ViewMode::List);
        imp.grid_view_button.set_active(view_mode == ViewMode::Grid);
        imp.merge_view_stack.set_visible_child_name(view_name);
        imp.organize_view_stack.set_visible_child_name(view_name);
        imp.extract_view_stack.set_visible_child_name(view_name);
    }

    fn active_tool_has_view_mode_content(&self) -> bool {
        let imp = self.imp();
        match imp.active_tool.get() {
            PdfTool::Merge => !imp.input_files.borrow().is_empty(),
            PdfTool::Organize => imp.organize_file.borrow().is_some(),
            PdfTool::Extract => imp.extract_file.borrow().is_some(),
            PdfTool::Compress | PdfTool::Split => false,
        }
    }

    fn update_all_views(&self) {
        self.update_view_mode();
        self.update_files_view();
        self.update_compress_view();
        self.update_organize_view();
        self.update_extract_view();
        self.update_split_view();
    }

    fn open_last_output(&self) {
        let imp = self.imp();
        let path = match imp.active_tool.get() {
            PdfTool::Merge => imp.last_output.borrow().clone(),
            PdfTool::Compress => imp.compress_last_output.borrow().clone(),
            PdfTool::Organize => imp.organize_last_output.borrow().clone(),
            PdfTool::Extract => imp.extract_last_output.borrow().clone(),
            PdfTool::Split => imp.split_last_output.borrow().clone(),
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
