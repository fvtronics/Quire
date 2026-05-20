use super::ui::icon_button;
use super::FoliosWindow;
use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};
use std::future::Future;
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

pub(super) fn show_backend_error(
    widget: &impl IsA<gtk::Widget>,
    error: &crate::pdf::PdfBackendError,
) {
    show_toast(widget, &backend_error_message(error));
}

pub(super) fn show_preview_error(
    widget: &impl IsA<gtk::Widget>,
    error: &crate::preview::PreviewError,
) {
    show_toast(widget, &preview_error_message(error));
}

pub(super) fn run_output_job<Widget, Operation, SetRunning, ClearOutput, StoreOutput, Refresh>(
    widget: Widget,
    operation: Operation,
    success_message: String,
    set_running: SetRunning,
    clear_output: ClearOutput,
    store_output: StoreOutput,
    refresh: Refresh,
) where
    Widget: IsA<gtk::Widget> + Clone + 'static,
    Operation: Future<Output = Result<std::path::PathBuf, crate::pdf::PdfBackendError>> + 'static,
    SetRunning: Fn(&Widget, bool) + 'static,
    ClearOutput: Fn(&Widget) + 'static,
    StoreOutput: Fn(&Widget, std::path::PathBuf) + 'static,
    Refresh: Fn(&Widget) + 'static,
{
    set_running(&widget, true);
    clear_output(&widget);
    refresh(&widget);

    glib::spawn_future_local(async move {
        let result = operation.await;
        set_running(&widget, false);

        match result {
            Ok(path) => {
                store_output(&widget, path);
                show_toast(&widget, &success_message);
            }
            Err(error) => {
                show_backend_error(&widget, &error);
            }
        }

        refresh(&widget);
    });
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
        eprintln!("Could not open output: {error}");
        show_toast(widget, &gettext("Could not open output"));
    }
}

pub(super) struct OrderedItemControls {
    pub up: gtk::Button,
    pub down: gtk::Button,
    pub rotate: gtk::Button,
    pub remove: gtk::Button,
}

pub(super) struct OrderedItemControlOptions {
    pub controls_sensitive: bool,
    pub can_move_up: bool,
    pub can_move_down: bool,
    pub can_remove: bool,
}

impl OrderedItemControls {
    pub(super) fn append_to_row(&self, row: &adw::ActionRow) {
        row.add_suffix(&self.up);
        row.add_suffix(&self.down);
        row.add_suffix(&self.rotate);
        row.add_suffix(&self.remove);
    }

    pub(super) fn append_to_box(&self, box_: &gtk::Box) {
        box_.append(&self.up);
        box_.append(&self.down);
        box_.append(&self.rotate);
        box_.append(&self.remove);
    }
}

pub(super) fn ordered_item_controls(
    options: OrderedItemControlOptions,
    on_move_up: impl Fn() + 'static,
    on_move_down: impl Fn() + 'static,
    on_rotate: impl Fn() + 'static,
    on_remove: impl Fn() + 'static,
) -> OrderedItemControls {
    let up = icon_button("go-up-symbolic", &gettext("Move Up"));
    up.set_sensitive(options.controls_sensitive && options.can_move_up);
    up.connect_clicked(move |_| on_move_up());

    let down = icon_button("go-down-symbolic", &gettext("Move Down"));
    down.set_sensitive(options.controls_sensitive && options.can_move_down);
    down.connect_clicked(move |_| on_move_down());

    let rotate = icon_button("object-rotate-right-symbolic", &gettext("Rotate Clockwise"));
    rotate.set_sensitive(options.controls_sensitive);
    rotate.connect_clicked(move |_| on_rotate());

    let remove = icon_button("edit-delete-symbolic", &gettext("Remove"));
    remove.set_sensitive(options.controls_sensitive && options.can_remove);
    remove.connect_clicked(move |_| on_remove());

    OrderedItemControls {
        up,
        down,
        rotate,
        remove,
    }
}

fn backend_error_message(error: &crate::pdf::PdfBackendError) -> String {
    match error {
        crate::pdf::PdfBackendError::NotEnoughInputs => gettext("Choose at least two PDFs"),
        crate::pdf::PdfBackendError::NoPagesSelected => gettext("Choose at least one page"),
        crate::pdf::PdfBackendError::OutputMatchesInput => {
            gettext("Choose a different output file")
        }
        crate::pdf::PdfBackendError::InvalidPageRange(message) => message.clone(),
        crate::pdf::PdfBackendError::Load { .. } => gettext("Could not open PDF"),
        crate::pdf::PdfBackendError::InvalidDocument(_) => gettext("Could not process PDF"),
        crate::pdf::PdfBackendError::Write(_) | crate::pdf::PdfBackendError::Save(_) => {
            gettext("Could not save PDF")
        }
        crate::pdf::PdfBackendError::WorkerStopped => gettext("Could not finish operation"),
    }
}

fn preview_error_message(_error: &crate::preview::PreviewError) -> String {
    gettext("Could not preview PDF")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::PdfBackendError;
    use crate::preview::PreviewError;
    use std::path::PathBuf;

    #[test]
    fn backend_errors_use_generic_user_messages() {
        assert_eq!(
            backend_error_message(&PdfBackendError::Load {
                path: PathBuf::from("broken.pdf"),
                message: "xref table exploded".to_string(),
            }),
            "Could not open PDF"
        );
        assert_eq!(
            backend_error_message(&PdfBackendError::InvalidDocument(
                "missing catalog".to_string()
            )),
            "Could not process PDF"
        );
        assert_eq!(
            backend_error_message(&PdfBackendError::Write("permission denied".to_string())),
            "Could not save PDF"
        );
        assert_eq!(
            backend_error_message(&PdfBackendError::Save(std::io::Error::other("disk full",))),
            "Could not save PDF"
        );
        assert_eq!(
            backend_error_message(&PdfBackendError::WorkerStopped),
            "Could not finish operation"
        );
    }

    #[test]
    fn backend_errors_keep_actionable_validation_messages() {
        assert_eq!(
            backend_error_message(&PdfBackendError::OutputMatchesInput),
            "Choose a different output file"
        );
        assert_eq!(
            backend_error_message(&PdfBackendError::InvalidPageRange(
                "Page 42 is not in this PDF.".to_string(),
            )),
            "Page 42 is not in this PDF."
        );
    }

    #[test]
    fn preview_errors_use_generic_user_messages() {
        assert_eq!(
            preview_error_message(&PreviewError::Load("poppler detail".to_string())),
            "Could not preview PDF"
        );
        assert_eq!(
            preview_error_message(&PreviewError::Render("cairo detail".to_string())),
            "Could not preview PDF"
        );
        assert_eq!(
            preview_error_message(&PreviewError::WorkerStopped),
            "Could not preview PDF"
        );
    }
}
