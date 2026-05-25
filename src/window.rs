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

mod compress;
mod extract;
mod merge;
mod metadata;
mod organize;
mod split;
mod state;
mod ui;
mod workspace;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum PdfTool {
    #[default]
    Merge,
    Compress,
    Organize,
    Extract,
    Split,
    Metadata,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum ViewMode {
    #[default]
    List,
    Grid,
}

const LIST_VIEW_NAME: &str = "list";
const GRID_VIEW_NAME: &str = "grid";

impl ViewMode {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::List => LIST_VIEW_NAME,
            Self::Grid => GRID_VIEW_NAME,
        }
    }
}

impl PdfTool {
    fn title(self) -> String {
        match self {
            Self::Merge => gettext("Merge PDFs"),
            Self::Compress => gettext("Compress PDF"),
            Self::Organize => gettext("Organize Pages"),
            Self::Extract => gettext("Extract Pages"),
            Self::Split => gettext("Split PDF"),
            Self::Metadata => gettext("Edit Metadata"),
        }
    }

    fn default_subtitle(self) -> String {
        match self {
            Self::Merge => gettext("No files selected"),
            Self::Compress | Self::Organize | Self::Extract | Self::Split | Self::Metadata => {
                gettext("No PDF selected")
            }
        }
    }
}

mod imp {
    use super::compress::CompressWorkspace;
    use super::extract::ExtractWorkspace;
    use super::merge::MergeWorkspace;
    use super::metadata::MetadataWorkspace;
    use super::organize::OrganizeWorkspace;
    use super::split::SplitWorkspace;
    use super::{PdfTool, ViewMode};
    use adw::subclass::prelude::*;
    use gtk::prelude::*;
    use gtk::{glib, TemplateChild};
    use std::cell::Cell;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/fvtronics/folios/window.ui")]
    pub struct FoliosWindow {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub navigation_split_view: TemplateChild<adw::NavigationSplitView>,
        #[template_child]
        pub sidebar_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub view_mode_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub content_title: TemplateChild<adw::WindowTitle>,
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
        pub metadata_tool_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub merge_workspace: TemplateChild<MergeWorkspace>,
        #[template_child]
        pub compress_workspace: TemplateChild<CompressWorkspace>,
        #[template_child]
        pub organize_workspace: TemplateChild<OrganizeWorkspace>,
        #[template_child]
        pub extract_workspace: TemplateChild<ExtractWorkspace>,
        #[template_child]
        pub split_workspace: TemplateChild<SplitWorkspace>,
        #[template_child]
        pub metadata_workspace: TemplateChild<MetadataWorkspace>,

        pub(super) active_tool: Cell<PdfTool>,
        pub(super) view_mode: Cell<ViewMode>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FoliosWindow {
        const NAME: &'static str = "FoliosWindow";
        type Type = super::FoliosWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            MergeWorkspace::static_type();
            CompressWorkspace::static_type();
            OrganizeWorkspace::static_type();
            ExtractWorkspace::static_type();
            SplitWorkspace::static_type();
            MetadataWorkspace::static_type();
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
            obj.setup_responsive_navigation();
            obj.switch_tool(PdfTool::Merge);
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
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native,
            gtk::Root, gtk::ShortcutManager, gio::ActionGroup, gio::ActionMap;
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

        let window = self.clone();
        imp.metadata_tool_row.connect_activated(move |_| {
            window.switch_tool(PdfTool::Metadata);
        });
    }

    fn setup_responsive_navigation(&self) {
        let imp = self.imp();
        let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            800.0,
            adw::LengthUnit::Sp,
        ));
        breakpoint.add_setter(
            &imp.navigation_split_view.get(),
            "collapsed",
            Some(&true.to_value()),
        );
        self.add_breakpoint(breakpoint);
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
        imp.metadata_workspace
            .set_visible(tool == PdfTool::Metadata);

        let selected_row: &gtk::ListBoxRow = match tool {
            PdfTool::Merge => imp.merge_tool_row.upcast_ref(),
            PdfTool::Compress => imp.compress_tool_row.upcast_ref(),
            PdfTool::Organize => imp.organize_tool_row.upcast_ref(),
            PdfTool::Extract => imp.extract_tool_row.upcast_ref(),
            PdfTool::Split => imp.split_tool_row.upcast_ref(),
            PdfTool::Metadata => imp.metadata_tool_row.upcast_ref(),
        };
        imp.sidebar_list.select_row(Some(selected_row));
        imp.navigation_split_view.set_show_content(true);
        self.set_content_title(&tool.title(), &tool.default_subtitle());
        self.update_active_workspace();
        self.update_view_mode();
    }

    fn set_content_title(&self, title: &str, subtitle: &str) {
        let imp = self.imp();
        imp.content_title.set_title(title);
        imp.content_title.set_subtitle(subtitle);
    }

    pub(super) fn set_tool_content_subtitle(&self, tool: PdfTool, subtitle: &str) {
        if self.imp().active_tool.get() == tool {
            self.set_content_title(&tool.title(), subtitle);
        }
    }

    fn update_active_workspace(&self) {
        let imp = self.imp();
        match imp.active_tool.get() {
            PdfTool::Merge => imp.merge_workspace.update_view(),
            PdfTool::Compress => imp.compress_workspace.update_view(),
            PdfTool::Organize => imp.organize_workspace.update_view(),
            PdfTool::Extract => imp.extract_workspace.update_view(),
            PdfTool::Split => imp.split_workspace.update_view(),
            PdfTool::Metadata => imp.metadata_workspace.update_view(),
        }
    }

    fn set_view_mode(&self, view_mode: ViewMode) {
        self.imp().view_mode.set(view_mode);
        self.update_view_mode();
    }

    pub(super) fn update_view_mode(&self) {
        let imp = self.imp();
        let view_mode = imp.view_mode.get();
        imp.view_mode_box
            .set_visible(self.active_tool_has_view_mode_content());
        imp.list_view_button.set_active(view_mode == ViewMode::List);
        imp.grid_view_button.set_active(view_mode == ViewMode::Grid);
        imp.merge_workspace.set_view_mode(view_mode);
        imp.organize_workspace.set_view_mode(view_mode);
        imp.extract_workspace.set_view_mode(view_mode);
    }

    fn active_tool_has_view_mode_content(&self) -> bool {
        let imp = self.imp();
        match imp.active_tool.get() {
            PdfTool::Merge => imp.merge_workspace.has_view_mode_content(),
            PdfTool::Compress | PdfTool::Split | PdfTool::Metadata => false,
            PdfTool::Organize => imp.organize_workspace.has_view_mode_content(),
            PdfTool::Extract => imp.extract_workspace.has_view_mode_content(),
        }
    }

    pub(super) fn show_toast(&self, message: &str) {
        let imp = self.imp();
        imp.toast_overlay.add_toast(adw::Toast::new(message));
    }
}
