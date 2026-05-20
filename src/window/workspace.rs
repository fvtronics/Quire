use super::FoliosWindow;
use adw::prelude::*;
use gtk::gio;
use std::path::Path;

pub(super) fn parent_window(widget: &impl IsA<gtk::Widget>) -> Option<gtk::Window> {
    widget.root().and_downcast::<gtk::Window>()
}

pub(super) fn show_toast(widget: &impl IsA<gtk::Widget>, message: &str) {
    let Some(window) = widget
        .root()
        .and_downcast::<gtk::Window>()
        .and_then(|window| window.downcast::<FoliosWindow>().ok())
    else {
        return;
    };

    {
        window.show_toast(message);
    }
}

pub(super) fn update_shell_view_mode(widget: &impl IsA<gtk::Widget>) {
    let Some(window) = widget
        .root()
        .and_downcast::<gtk::Window>()
        .and_then(|window| window.downcast::<FoliosWindow>().ok())
    else {
        return;
    };

    window.update_view_mode();
}

pub(super) fn open_output(widget: &impl IsA<gtk::Widget>, path: &Path) {
    let file = gio::File::for_path(path);
    if let Err(error) =
        gio::AppInfo::launch_default_for_uri(file.uri().as_str(), None::<&gio::AppLaunchContext>)
    {
        show_toast(widget, &error.to_string());
    }
}
