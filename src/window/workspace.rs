use super::ui::{ask_pdf_password, icon_button, PasswordPromptReason};
use super::{FoliosWindow, PdfTool};
use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::rc::Rc;

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

pub(super) fn update_shell_title(widget: &impl IsA<gtk::Widget>, tool: PdfTool, subtitle: &str) {
    let Some(window) = widget
        .root()
        .and_downcast::<gtk::Window>()
        .and_then(|window| window.downcast::<FoliosWindow>().ok())
    else {
        return;
    };

    window.set_tool_content_subtitle(tool, subtitle);
}

pub(super) fn show_backend_error(
    widget: &impl IsA<gtk::Widget>,
    error: &crate::pdf::PdfBackendError,
) {
    eprintln!("PDF backend error: {error}");
    show_toast(widget, &backend_error_message(error));
}

pub(super) fn show_preview_error(
    widget: &impl IsA<gtk::Widget>,
    error: &crate::preview::PreviewError,
) {
    eprintln!("PDF preview error: {error}");
    show_toast(widget, &preview_error_message(error));
}

pub(super) enum PdfLoadResult<T> {
    Loaded { output: T, password: Option<String> },
    Failed(crate::preview::PreviewError),
    Cancelled,
}

pub(super) fn show_pdf_load_error(
    widget: &impl IsA<gtk::Widget>,
    error: &crate::preview::PreviewError,
) {
    show_preview_error(widget, error);
}

pub(super) async fn load_processable_pdf<T, Load, Operation>(
    parent: &gtk::Window,
    path: &Path,
    mut load: Load,
) -> PdfLoadResult<T>
where
    Load: FnMut(Option<String>) -> Operation,
    Operation: Future<Output = Result<T, crate::preview::PreviewError>>,
{
    let mut password = None;

    loop {
        match load(password.clone()).await {
            Ok(output) => return PdfLoadResult::Loaded { output, password },
            Err(crate::preview::PreviewError::PasswordRequired) => {
                password = ask_pdf_password(parent, path, PasswordPromptReason::Required).await;
                if password.is_none() {
                    return PdfLoadResult::Cancelled;
                }
            }
            Err(crate::preview::PreviewError::InvalidPassword) => {
                password =
                    ask_pdf_password(parent, path, PasswordPromptReason::InvalidPassword).await;
                if password.is_none() {
                    return PdfLoadResult::Cancelled;
                }
            }
            Err(error) => return PdfLoadResult::Failed(error),
        }
    }
}

pub(super) struct SinglePdfLoadHandlers<Begin, Store, Fail, Refresh> {
    pub begin_loading: Begin,
    pub store_loaded: Store,
    pub finish_loading_failed: Fail,
    pub refresh: Refresh,
}

pub(super) fn load_single_processable_pdf<Widget, Load, Operation, Begin, Store, Fail, Refresh, T>(
    widget: Widget,
    parent: gtk::Window,
    path: PathBuf,
    mut load: Load,
    handlers: SinglePdfLoadHandlers<Begin, Store, Fail, Refresh>,
) where
    Widget: IsA<gtk::Widget> + Clone + 'static,
    Load: FnMut(PathBuf, Option<String>) -> Operation + 'static,
    Operation: Future<Output = Result<T, crate::preview::PreviewError>> + 'static,
    Begin: Fn(&Widget) + 'static,
    Store: Fn(&Widget, PathBuf, Option<String>, T) + 'static,
    Fail: Fn(&Widget) + 'static,
    Refresh: Fn(&Widget) + 'static,
    T: 'static,
{
    (handlers.begin_loading)(&widget);
    (handlers.refresh)(&widget);

    glib::spawn_future_local(async move {
        let result =
            load_processable_pdf(&parent, &path, |password| load(path.clone(), password)).await;

        match result {
            PdfLoadResult::Loaded { output, password } => {
                (handlers.store_loaded)(&widget, path, password, output);
            }
            PdfLoadResult::Failed(error) => {
                (handlers.finish_loading_failed)(&widget);
                show_pdf_load_error(&widget, &error);
            }
            PdfLoadResult::Cancelled => {
                (handlers.finish_loading_failed)(&widget);
            }
        }

        (handlers.refresh)(&widget);
    });
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

struct ActionOptionRow {
    title: String,
    on_activate: Box<dyn Fn() + 'static>,
}

struct SwitchOptionRow {
    active: bool,
    on_active_changed: Box<dyn Fn(bool) + 'static>,
}

pub(super) struct AdvancedOptionsMenu {
    rotate: Option<ActionOptionRow>,
    normalize_page_size: Option<SwitchOptionRow>,
    modern_pdf: Box<dyn Fn(bool) + 'static>,
    remove_metadata: Box<dyn Fn(bool) + 'static>,
}

impl AdvancedOptionsMenu {
    pub(super) fn new(
        on_modern_pdf: impl Fn(bool) + 'static,
        on_remove_metadata: impl Fn(bool) + 'static,
    ) -> Self {
        Self {
            rotate: None,
            normalize_page_size: None,
            modern_pdf: Box::new(on_modern_pdf),
            remove_metadata: Box::new(on_remove_metadata),
        }
    }

    pub(super) fn with_rotate(mut self, title: String, on_rotate: impl Fn() + 'static) -> Self {
        self.rotate = Some(ActionOptionRow {
            title,
            on_activate: Box::new(on_rotate),
        });
        self
    }

    pub(super) fn with_normalize_page_size(
        mut self,
        active: bool,
        on_normalize_page_size: impl Fn(bool) + 'static,
    ) -> Self {
        self.normalize_page_size = Some(SwitchOptionRow {
            active,
            on_active_changed: Box::new(on_normalize_page_size),
        });
        self
    }
}

pub(super) fn output_option_callback<Widget, Update, ClearOutput, Refresh>(
    widget: Widget,
    update: Update,
    clear_output: ClearOutput,
    refresh: Refresh,
) -> impl Fn(bool) + 'static
where
    Widget: IsA<gtk::Widget> + Clone + 'static,
    Update: Fn(&Widget, bool) + 'static,
    ClearOutput: Fn(&Widget) + 'static,
    Refresh: Fn(&Widget) + 'static,
{
    move |active| {
        update(&widget, active);
        clear_output(&widget);
        refresh(&widget);
    }
}

pub(super) fn setup_advanced_options_menu(
    button: &gtk::MenuButton,
    options: &super::state::SaveOptionsState,
    menu: AdvancedOptionsMenu,
) {
    let popover = gtk::Popover::new();
    popover.add_css_class("menu");

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);

    if let Some(rotate) = menu.rotate {
        let rotate_row = adw::ActionRow::builder()
            .title(rotate.title)
            .activatable(true)
            .build();
        rotate_row.connect_activated(move |_| (rotate.on_activate)());
        list.append(&rotate_row);
    }

    let modern_pdf = adw::SwitchRow::builder()
        .title(gettext("Modern PDF Format"))
        .tooltip_text(gettext("Save with PDF 1.5 object streams"))
        .active(options.modern_pdf())
        .build();
    modern_pdf.connect_active_notify(move |row| (menu.modern_pdf)(row.is_active()));
    let remove_metadata = adw::SwitchRow::builder()
        .title(gettext("Remove Metadata"))
        .tooltip_text(gettext("Remove existing metadata before saving"))
        .active(options.remove_metadata())
        .build();
    remove_metadata.connect_active_notify(move |row| (menu.remove_metadata)(row.is_active()));

    list.append(&modern_pdf);

    if let Some(normalize_page_size_option) = menu.normalize_page_size {
        let normalize_page_size = adw::SwitchRow::builder()
            .title(gettext("Normalize Page Size"))
            .tooltip_text(gettext("Resize output pages to the largest page size"))
            .active(normalize_page_size_option.active)
            .build();
        normalize_page_size.connect_active_notify(move |row| {
            (normalize_page_size_option.on_active_changed)(row.is_active());
        });
        list.append(&normalize_page_size);
    }

    list.append(&remove_metadata);

    popover.set_child(Some(&list));
    button.set_popover(Some(&popover));
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

#[derive(Clone, Copy, Default)]
pub(super) struct CollectionScrollPosition {
    list: f64,
    grid: f64,
}

pub(super) fn collection_scroll_position(
    list: &gtk::ScrolledWindow,
    grid: &gtk::ScrolledWindow,
) -> CollectionScrollPosition {
    CollectionScrollPosition {
        list: list.vadjustment().value(),
        grid: grid.vadjustment().value(),
    }
}

pub(super) fn restore_collection_scroll_position(
    list: &gtk::ScrolledWindow,
    grid: &gtk::ScrolledWindow,
    position: CollectionScrollPosition,
) {
    let list_adjustment = list.vadjustment();
    let grid_adjustment = grid.vadjustment();
    restore_adjustment(&list_adjustment, position.list);
    restore_adjustment(&grid_adjustment, position.grid);

    glib::idle_add_local_once(move || {
        restore_adjustment(&list_adjustment, position.list);
        restore_adjustment(&grid_adjustment, position.grid);
    });
}

pub(super) fn preserve_collection_scroll_position(
    list: &gtk::ScrolledWindow,
    grid: &gtk::ScrolledWindow,
    update: impl FnOnce(),
) {
    let position = collection_scroll_position(list, grid);
    update();
    restore_collection_scroll_position(list, grid, position);
}

fn restore_adjustment(adjustment: &gtk::Adjustment, value: f64) {
    let upper = (adjustment.upper() - adjustment.page_size()).max(adjustment.lower());
    adjustment.set_value(value.clamp(adjustment.lower(), upper));
}

pub(super) fn replace_collection_item(
    list: &gtk::ListBox,
    grid: &gtk::FlowBox,
    index: usize,
    row: &impl IsA<gtk::Widget>,
    tile: &impl IsA<gtk::Widget>,
) {
    let index = index as i32;
    if let Some(old_row) = list.row_at_index(index) {
        list.remove(&old_row);
        list.insert(row, index);
    }
    if let Some(old_tile) = grid.child_at_index(index) {
        grid.remove(&old_tile);
        grid.insert(tile, index);
    }
}

pub(super) struct OrderedItemControls {
    pub up: gtk::Button,
    pub down: gtk::Button,
    pub rotate: gtk::Button,
    pub remove: gtk::Button,
}

#[derive(Clone, Copy)]
pub(super) struct OrderedItemControlOptions {
    pub can_move_up: bool,
    pub can_move_down: bool,
    pub can_remove: bool,
}

pub(super) struct OrderedItemActions {
    options: OrderedItemControlOptions,
    move_up: Rc<dyn Fn() + 'static>,
    move_down: Rc<dyn Fn() + 'static>,
    rotate: Rc<dyn Fn() + 'static>,
    remove: Rc<dyn Fn() + 'static>,
}

impl OrderedItemActions {
    pub(super) fn new(
        options: OrderedItemControlOptions,
        on_move_up: impl Fn() + 'static,
        on_move_down: impl Fn() + 'static,
        on_rotate: impl Fn() + 'static,
        on_remove: impl Fn() + 'static,
    ) -> Self {
        Self {
            options,
            move_up: Rc::new(on_move_up),
            move_down: Rc::new(on_move_down),
            rotate: Rc::new(on_rotate),
            remove: Rc::new(on_remove),
        }
    }
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

pub(super) fn ordered_item_controls(actions: &OrderedItemActions) -> OrderedItemControls {
    let up = icon_button("go-up-symbolic", &gettext("Move Up"));
    up.set_sensitive(actions.options.can_move_up);
    let move_up = actions.move_up.clone();
    up.connect_clicked(move |_| (move_up)());

    let down = icon_button("go-down-symbolic", &gettext("Move Down"));
    down.set_sensitive(actions.options.can_move_down);
    let move_down = actions.move_down.clone();
    down.connect_clicked(move |_| (move_down)());

    let rotate = icon_button("object-rotate-right-symbolic", &gettext("Rotate Clockwise"));
    let rotate_action = actions.rotate.clone();
    rotate.connect_clicked(move |_| (rotate_action)());

    let remove = icon_button("edit-delete-symbolic", &gettext("Remove"));
    remove.set_sensitive(actions.options.can_remove);
    let remove_action = actions.remove.clone();
    remove.connect_clicked(move |_| (remove_action)());

    OrderedItemControls {
        up,
        down,
        rotate,
        remove,
    }
}

pub(super) fn ordered_item_context_menu_items(
    actions: &OrderedItemActions,
) -> Vec<ContextMenuItem> {
    vec![
        ContextMenuItem::from_action(
            "move-up",
            gettext("Move Up"),
            actions.options.can_move_up,
            actions.move_up.clone(),
        ),
        ContextMenuItem::from_action(
            "move-down",
            gettext("Move Down"),
            actions.options.can_move_down,
            actions.move_down.clone(),
        ),
        ContextMenuItem::from_action(
            "rotate",
            gettext("Rotate Clockwise"),
            true,
            actions.rotate.clone(),
        ),
        ContextMenuItem::from_action(
            "remove",
            gettext("Remove"),
            actions.options.can_remove,
            actions.remove.clone(),
        ),
    ]
}

pub(super) struct ContextMenuItem {
    name: &'static str,
    label: String,
    sensitive: bool,
    on_activate: Rc<dyn Fn() + 'static>,
}

impl ContextMenuItem {
    pub(super) fn new(
        name: &'static str,
        label: String,
        sensitive: bool,
        on_activate: impl Fn() + 'static,
    ) -> Self {
        Self::from_action(name, label, sensitive, Rc::new(on_activate))
    }

    pub(super) fn from_action(
        name: &'static str,
        label: String,
        sensitive: bool,
        on_activate: Rc<dyn Fn() + 'static>,
    ) -> Self {
        Self {
            name,
            label,
            sensitive,
            on_activate,
        }
    }
}

pub(super) fn add_item_context_menu(widget: &impl IsA<gtk::Widget>, items: Vec<ContextMenuItem>) {
    let menu = gio::Menu::new();
    let actions = gio::SimpleActionGroup::new();

    for item in items {
        menu.append(Some(&item.label), Some(&format!("item.{}", item.name)));
        add_context_menu_action(&actions, item);
    }
    widget.insert_action_group("item", Some(&actions));

    let popover = gtk::PopoverMenu::from_model(Some(&menu));
    popover.set_has_arrow(false);
    popover.set_position(gtk::PositionType::Right);
    popover.set_parent(widget);

    let gesture = gtk::GestureClick::new();
    gesture.set_button(gtk::gdk::BUTTON_SECONDARY);
    let popover_for_click = popover.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        let bounds = gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
        popover_for_click.set_pointing_to(Some(&bounds));
        popover_for_click.popup();
        gesture.set_state(gtk::EventSequenceState::Claimed);
    });
    widget.add_controller(gesture);

    let popover_for_realize = popover.clone();
    widget.connect_realize(move |widget| {
        if popover_for_realize.parent().is_none() {
            popover_for_realize.set_parent(widget);
        }
    });

    widget.connect_unrealize(move |_| {
        popover.unparent();
    });
}

fn add_context_menu_action(actions: &gio::SimpleActionGroup, item: ContextMenuItem) {
    let action = gio::SimpleAction::new(item.name, None);
    action.set_enabled(item.sensitive);
    action.connect_activate(move |_, _| {
        (item.on_activate)();
    });
    actions.add_action(&action);
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
        crate::pdf::PdfBackendError::PasswordRequired { .. } => gettext("Password required"),
        crate::pdf::PdfBackendError::InvalidPassword { .. } => gettext("Invalid password"),
        crate::pdf::PdfBackendError::InvalidDocument(_) => gettext("Could not process PDF"),
        crate::pdf::PdfBackendError::Write(_) | crate::pdf::PdfBackendError::Save(_) => {
            gettext("Could not save PDF")
        }
        crate::pdf::PdfBackendError::WorkerStopped => gettext("Could not finish operation"),
    }
}

fn preview_error_message(_error: &crate::preview::PreviewError) -> String {
    match _error {
        crate::preview::PreviewError::PasswordRequired => gettext("Password required"),
        crate::preview::PreviewError::InvalidPassword => gettext("Invalid password"),
        _ => gettext("Could not preview PDF"),
    }
}

#[cfg(test)]
mod tests {
    use super::{backend_error_message, preview_error_message};
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
            backend_error_message(&PdfBackendError::PasswordRequired {
                path: PathBuf::from("locked.pdf"),
            }),
            "Password required"
        );
        assert_eq!(
            backend_error_message(&PdfBackendError::InvalidPassword {
                path: PathBuf::from("locked.pdf"),
            }),
            "Invalid password"
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
            preview_error_message(&PreviewError::PasswordRequired),
            "Password required"
        );
        assert_eq!(
            preview_error_message(&PreviewError::InvalidPassword),
            "Invalid password"
        );
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
