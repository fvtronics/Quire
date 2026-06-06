use crate::image::argb32_surface_texture;
use adw::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

const LIST_COLLECTION_PREVIEW_WIDTH: i32 = 128;
const LIST_COLLECTION_PREVIEW_HEIGHT: i32 = 128;
const LIST_COLLECTION_PREVIEW_MIN_SIZE: i32 = 72;
const GRID_COLLECTION_PREVIEW_WIDTH: i32 = 200;
const GRID_COLLECTION_PREVIEW_HEIGHT: i32 = 220;
const GRID_COLLECTION_PREVIEW_MIN_WIDTH: i32 = 120;
const GRID_COLLECTION_PREVIEW_MIN_HEIGHT: i32 = 132;
const SINGLE_FILE_PREVIEW_MIN_WIDTH: i32 = 40;
const SINGLE_FILE_PREVIEW_MIN_HEIGHT: i32 = 54;
const PREVIEW_TILE_MIN_WIDTH: i32 = 150;
const ENTRY_VALIDATION_DELAY: Duration = Duration::from_secs(1);

pub(super) fn pdf_filters() -> gio::ListStore {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&gettext("PDF Documents")));
    filter.add_mime_type("application/pdf");
    filter.add_pattern("*.pdf");

    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    filters
}

#[allow(deprecated)]
pub(super) fn image_filters() -> gio::ListStore {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&gettext("Images")));
    filter.add_pixbuf_formats();

    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    filters
}

pub(super) async fn open_pdf_file(
    parent: &impl IsA<gtk::Window>,
    title: &str,
    accept_label: &str,
) -> Option<PathBuf> {
    pdf_file_dialog(title, accept_label, None)
        .open_future(Some(parent))
        .await
        .ok()
        .and_then(|file| file.path())
}

pub(super) async fn open_image_file(
    parent: &impl IsA<gtk::Window>,
    title: &str,
    accept_label: &str,
) -> Option<PathBuf> {
    gtk::FileDialog::builder()
        .title(title)
        .accept_label(accept_label)
        .modal(true)
        .filters(&image_filters())
        .build()
        .open_future(Some(parent))
        .await
        .ok()
        .and_then(|file| file.path())
}

pub(super) async fn open_pdf_files(
    parent: &impl IsA<gtk::Window>,
    title: &str,
    accept_label: &str,
) -> Vec<PathBuf> {
    let Ok(files) = pdf_file_dialog(title, accept_label, None)
        .open_multiple_future(Some(parent))
        .await
    else {
        return Vec::new();
    };

    (0..files.n_items())
        .filter_map(|position| files.item(position).and_downcast::<gio::File>())
        .filter_map(|file| file.path())
        .collect()
}

pub(super) async fn save_pdf_file(
    parent: &impl IsA<gtk::Window>,
    title: &str,
    accept_label: &str,
    initial_name: &str,
) -> Option<PathBuf> {
    pdf_file_dialog(title, accept_label, Some(initial_name))
        .save_future(Some(parent))
        .await
        .ok()
        .and_then(|file| file.path())
}

pub(super) fn output_pdf_name(input_file: &Path, action: &str) -> String {
    input_file
        .file_stem()
        .and_then(|name| name.to_str())
        .map(|name| format!("{name}_{action}.pdf"))
        .unwrap_or_else(|| format!("{action}.pdf"))
}

pub(super) async fn select_folder(
    parent: &impl IsA<gtk::Window>,
    title: &str,
    accept_label: &str,
) -> Option<PathBuf> {
    gtk::FileDialog::builder()
        .title(title)
        .accept_label(accept_label)
        .modal(true)
        .build()
        .select_folder_future(Some(parent))
        .await
        .ok()
        .and_then(|folder| folder.path())
}

pub(super) enum PasswordPromptReason {
    Required,
    InvalidPassword,
}

pub(super) async fn ask_pdf_password(
    parent: &impl IsA<gtk::Widget>,
    path: &Path,
    reason: PasswordPromptReason,
) -> Option<String> {
    let builder = gtk::Builder::from_resource("/com/fvtronics/Quire/password-dialog.ui");
    let dialog: adw::AlertDialog = builder
        .object("password_dialog")
        .expect("password dialog resource should define password_dialog");
    let entry: gtk::PasswordEntry = builder
        .object("password_entry")
        .expect("password dialog resource should define password_entry");
    let error_message: gtk::Label = builder
        .object("error_message")
        .expect("password dialog resource should define error_message");

    let body = gettext("Enter the password for {}.").replace("{}", file_title(path));
    let body = glib::markup_escape_text(&body);
    dialog.set_body(&body);
    error_message.set_visible(matches!(reason, PasswordPromptReason::InvalidPassword));

    let dialog_for_entry = dialog.clone();
    entry.connect_changed(move |entry| {
        dialog_for_entry.set_response_enabled("unlock", !entry.text().is_empty());
    });

    let response = dialog.choose_future(Some(parent)).await;
    if response.as_str() == "unlock" {
        Some(entry.text().to_string())
    } else {
        None
    }
}

pub(super) fn icon_button(icon_name: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon_name)
        .tooltip_text(tooltip)
        .valign(gtk::Align::Center)
        .build();
    button.add_css_class("flat");
    button
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum EntryValidationDisplay {
    #[default]
    Hidden,
    StyleOnly,
    Message,
}

#[derive(Debug, Default)]
pub(super) struct DelayedEntryValidationState {
    display: Cell<EntryValidationDisplay>,
}

impl DelayedEntryValidationState {
    pub(super) fn reset(&self) {
        self.display.set(EntryValidationDisplay::Hidden);
    }

    fn show_style(&self) {
        self.display.set(EntryValidationDisplay::StyleOnly);
    }

    fn show_message(&self) {
        self.display.set(EntryValidationDisplay::Message);
    }

    pub(super) fn display(&self, has_error: bool) -> EntryValidationDisplay {
        if has_error {
            self.display.get()
        } else {
            EntryValidationDisplay::Hidden
        }
    }
}

pub(super) struct EntryValidation<'a> {
    entry: &'a adw::EntryRow,
    error_row: &'a gtk::ListBoxRow,
    error_label: &'a gtk::Label,
}

impl<'a> EntryValidation<'a> {
    pub(super) fn new(
        entry: &'a adw::EntryRow,
        error_row: &'a gtk::ListBoxRow,
        error_label: &'a gtk::Label,
    ) -> Self {
        Self {
            entry,
            error_row,
            error_label,
        }
    }

    pub(super) fn set_error(&self, display: EntryValidationDisplay, message: &str) {
        if display != EntryValidationDisplay::Hidden {
            self.entry.add_css_class("error");
            self.entry
                .update_property(&[gtk::accessible::Property::Description(message)]);
            self.entry.update_state(&[gtk::accessible::State::Invalid(
                gtk::AccessibleInvalidState::True,
            )]);
            self.error_label
                .set_label(if display == EntryValidationDisplay::Message {
                    message
                } else {
                    ""
                });
            self.error_row
                .set_visible(display == EntryValidationDisplay::Message);
        } else {
            self.entry.remove_css_class("error");
            self.entry
                .reset_property(gtk::AccessibleProperty::Description);
            self.entry.update_state(&[gtk::accessible::State::Invalid(
                gtk::AccessibleInvalidState::False,
            )]);
            self.error_label.set_label("");
            self.error_row.set_visible(false);
        }
    }
}

pub(super) fn connect_delayed_entry_validation(
    entry: &adw::EntryRow,
    validation_state: Rc<DelayedEntryValidationState>,
    changed: impl Fn() + 'static,
    refresh: impl Fn() + 'static,
) {
    let pending_validation = Rc::new(RefCell::new(None::<glib::SourceId>));
    let refresh = Rc::new(refresh);

    let pending_validation_for_changed = pending_validation.clone();
    let validation_state_for_changed = validation_state.clone();
    let refresh_for_changed = refresh.clone();
    entry.connect_changed(move |_| {
        if let Some(source_id) = pending_validation_for_changed.borrow_mut().take() {
            source_id.remove();
        }

        validation_state_for_changed.reset();
        changed();

        let pending_validation_for_timeout = pending_validation_for_changed.clone();
        let validation_state_for_timeout = validation_state_for_changed.clone();
        let refresh = refresh_for_changed.clone();
        let source_id = glib::timeout_add_local_once(ENTRY_VALIDATION_DELAY, move || {
            pending_validation_for_timeout.borrow_mut().take();
            validation_state_for_timeout.show_style();
            refresh();
        });
        pending_validation_for_changed
            .borrow_mut()
            .replace(source_id);
    });

    let focus = gtk::EventControllerFocus::new();
    let pending_validation_for_focus = pending_validation.clone();
    let validation_state_for_focus = validation_state.clone();
    let refresh_for_focus = refresh.clone();
    focus.connect_leave(move |_| {
        if let Some(source_id) = pending_validation_for_focus.borrow_mut().take() {
            source_id.remove();
        }

        validation_state_for_focus.show_message();
        refresh_for_focus();
    });
    entry.add_controller(focus);
}

pub(super) fn page_ranges_error_message() -> String {
    gettext("Enter page ranges like 1,3-5,8.")
}

pub(super) fn page_numbers_error_message() -> String {
    gettext("Enter pages like 2,4,7.")
}

pub(super) fn page_count_error_message() -> String {
    gettext("Enter a page count of 1 or more.")
}

pub(super) fn pdf_file_row(path: &Path, subtitle: String) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(file_title(path))
        .subtitle(subtitle)
        .title_lines(1)
        .subtitle_lines(1)
        .focusable(false)
        .build();

    row.add_prefix(&gtk::Image::from_icon_name("view-paged-symbolic"));
    row
}

pub(super) fn file_title(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("PDF")
}

pub(super) fn file_subtitle(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(metadata) => format_size(metadata.len()),
        Err(_) => gettext("Size unavailable"),
    }
}

pub(super) fn preview_picture(preview: &crate::preview::PagePreview) -> gtk::Picture {
    rotated_preview_picture(preview, 0)
}

pub(super) fn rotated_preview_picture(
    preview: &crate::preview::PagePreview,
    rotation: i64,
) -> gtk::Picture {
    match rotated_preview_texture(preview, rotation).map(|texture| texture_picture(&texture)) {
        Some(picture) => picture,
        None => preview_picture_fallback(),
    }
}

pub(super) fn list_preview_widget(
    preview: Option<&crate::preview::PagePreview>,
    rotation: i64,
) -> gtk::Widget {
    collection_preview_widget(
        preview,
        rotation,
        LIST_COLLECTION_PREVIEW_WIDTH,
        LIST_COLLECTION_PREVIEW_HEIGHT,
    )
}

pub(super) fn blank_list_preview_widget(
    source_preview: Option<&crate::preview::PagePreview>,
    rotation: i64,
) -> gtk::Widget {
    collection_blank_preview_widget(
        source_preview,
        rotation,
        LIST_COLLECTION_PREVIEW_WIDTH,
        LIST_COLLECTION_PREVIEW_HEIGHT,
    )
}

pub(super) fn single_file_preview_widget(
    preview: Option<&crate::preview::PagePreview>,
) -> gtk::Widget {
    if let Some(preview) = preview {
        let picture = preview_picture(preview);
        picture.set_size_request(
            SINGLE_FILE_PREVIEW_MIN_WIDTH,
            SINGLE_FILE_PREVIEW_MIN_HEIGHT,
        );
        picture.upcast()
    } else {
        let placeholder = gtk::Image::from_icon_name("view-paged-symbolic");
        placeholder.set_size_request(
            SINGLE_FILE_PREVIEW_MIN_WIDTH,
            SINGLE_FILE_PREVIEW_MIN_HEIGHT,
        );
        placeholder.upcast()
    }
}

pub(super) fn preview_tile() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .width_request(PREVIEW_TILE_MIN_WIDTH)
        .build()
}

pub(super) fn tile_preview_widget(
    preview: Option<&crate::preview::PagePreview>,
    rotation: i64,
) -> gtk::Widget {
    collection_preview_widget(
        preview,
        rotation,
        GRID_COLLECTION_PREVIEW_WIDTH,
        GRID_COLLECTION_PREVIEW_HEIGHT,
    )
}

pub(super) fn blank_tile_preview_widget(
    source_preview: Option<&crate::preview::PagePreview>,
    rotation: i64,
) -> gtk::Widget {
    collection_blank_preview_widget(
        source_preview,
        rotation,
        GRID_COLLECTION_PREVIEW_WIDTH,
        GRID_COLLECTION_PREVIEW_HEIGHT,
    )
}

fn collection_preview_widget(
    preview: Option<&crate::preview::PagePreview>,
    rotation: i64,
    width: i32,
    height: i32,
) -> gtk::Widget {
    let child: gtk::Widget = if let Some(preview) = preview {
        let picture = match rotated_preview_texture(preview, rotation)
            .map(|texture| texture_picture(&texture))
        {
            Some(picture) => picture,
            None => preview_picture_fallback(),
        };
        picture.upcast()
    } else {
        collection_preview_placeholder(width, height)
    };

    collection_preview_slot(width, height, &child)
}

fn collection_blank_preview_widget(
    source_preview: Option<&crate::preview::PagePreview>,
    rotation: i64,
    width: i32,
    height: i32,
) -> gtk::Widget {
    let source_size = source_preview
        .map(|preview| rotated_preview_size(preview, rotation))
        .and_then(|size| fit_size(size.0, size.1, width, height))
        .unwrap_or_else(|| blank_page_fallback_size(width, height, rotation));
    let picture = match blank_page_texture(source_size.0, source_size.1) {
        Some(texture) => gtk::Picture::for_paintable(&texture),
        None => preview_picture_fallback(),
    };
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Contain);
    collection_preview_slot(width, height, &picture.upcast())
}

fn collection_preview_slot(width: i32, height: i32, child: &gtk::Widget) -> gtk::Widget {
    let slot = gtk::AspectFrame::new(0.5, 0.5, width as f32 / height as f32, false);
    let (request_width, request_height) = collection_preview_size_request(width, height);
    slot.set_size_request(request_width, request_height);
    slot.set_halign(gtk::Align::Center);
    slot.set_valign(gtk::Align::Center);
    slot.set_child(Some(child));
    slot.upcast()
}

fn collection_preview_placeholder(width: i32, height: i32) -> gtk::Widget {
    let placeholder = gtk::Image::from_icon_name("view-paged-symbolic");
    placeholder.set_pixel_size((width.min(height) / 2).max(16));
    let (request_width, request_height) = collection_preview_size_request(width, height);
    placeholder.set_size_request(request_width, request_height);
    placeholder.upcast()
}

fn collection_preview_size_request(width: i32, height: i32) -> (i32, i32) {
    if width <= LIST_COLLECTION_PREVIEW_WIDTH {
        (
            LIST_COLLECTION_PREVIEW_MIN_SIZE,
            LIST_COLLECTION_PREVIEW_MIN_SIZE,
        )
    } else {
        (
            GRID_COLLECTION_PREVIEW_MIN_WIDTH,
            GRID_COLLECTION_PREVIEW_MIN_HEIGHT.min(height),
        )
    }
}

fn blank_page_texture(width: i32, height: i32) -> Option<gtk::gdk::Texture> {
    let mut surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, width.max(1), height.max(1)).ok()?;
    let context = cairo::Context::new(&surface).ok()?;

    context.set_source_rgb(1.0, 1.0, 1.0);
    context.paint().ok()?;
    context.set_source_rgba(0.0, 0.0, 0.0, 0.25);
    context.set_line_width(1.0);
    context.rectangle(0.5, 0.5, width as f64 - 1.0, height as f64 - 1.0);
    context.stroke().ok()?;
    drop(context);

    argb32_surface_texture(&mut surface)
}

fn fit_size(width: i32, height: i32, max_width: i32, max_height: i32) -> Option<(i32, i32)> {
    if width <= 0 || height <= 0 || max_width <= 0 || max_height <= 0 {
        return None;
    }

    let scale = (max_width as f64 / width as f64).min(max_height as f64 / height as f64);
    Some((
        (width as f64 * scale).round().max(1.0) as i32,
        (height as f64 * scale).round().max(1.0) as i32,
    ))
}

fn blank_page_fallback_size(width: i32, height: i32, rotation: i64) -> (i32, i32) {
    let page_width = width;
    let page_height = (width as f64 * std::f64::consts::SQRT_2).ceil() as i32;
    let (page_width, page_height) = crate::image::rotated_size(page_width, page_height, rotation);
    fit_size(page_width, page_height, width, height).unwrap_or((width, height))
}

fn rotated_preview_texture(
    preview: &crate::preview::PagePreview,
    rotation: i64,
) -> Option<gtk::gdk::Texture> {
    preview.image.rotated(rotation)?.texture()
}

fn rotated_preview_size(preview: &crate::preview::PagePreview, rotation: i64) -> (i32, i32) {
    preview.image.rotated_size(rotation)
}

pub(super) fn texture_picture(texture: &gtk::gdk::Texture) -> gtk::Picture {
    let picture = gtk::Picture::for_paintable(texture);
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture
}

fn preview_picture_fallback() -> gtk::Picture {
    let picture = gtk::Picture::new();
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture
}

pub(super) fn tile_label(text: impl AsRef<str>) -> gtk::Label {
    gtk::Label::builder()
        .label(text.as_ref())
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build()
}

pub(super) fn dim_tile_label(text: impl AsRef<str>) -> gtk::Label {
    let label = tile_label(text);
    label.set_hexpand(true);
    label.add_css_class("dim-label");
    label
}

pub(super) fn tile_controls() -> gtk::Box {
    let controls = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    let key = gtk::EventControllerKey::new();
    key.set_propagation_phase(gtk::PropagationPhase::Capture);
    let controls_for_key = controls.clone();
    key.connect_key_pressed(move |_, key, _, modifiers| {
        let Some(direction) = tile_control_focus_direction(key, modifiers) else {
            return glib::Propagation::Proceed;
        };

        if controls_for_key.child_focus(direction) {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    controls.add_controller(key);

    controls
}

pub(super) fn clear_box(box_: &gtk::Box) {
    loop {
        let Some(child) = box_.first_child() else {
            break;
        };
        box_.remove(&child);
    }
}

pub(super) fn format_page_ranges(pages: &[u32]) -> String {
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

pub(super) fn page_count_label(count: usize) -> String {
    ngettext("1 page", "{} pages", count as u32).replace("{}", &count.to_string())
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

fn tile_control_focus_direction(
    key: gtk::gdk::Key,
    modifiers: gtk::gdk::ModifierType,
) -> Option<gtk::DirectionType> {
    let shortcut_modifiers = gtk::gdk::ModifierType::SHIFT_MASK
        | gtk::gdk::ModifierType::CONTROL_MASK
        | gtk::gdk::ModifierType::ALT_MASK
        | gtk::gdk::ModifierType::SUPER_MASK
        | gtk::gdk::ModifierType::HYPER_MASK
        | gtk::gdk::ModifierType::META_MASK;
    if modifiers.intersects(shortcut_modifiers) {
        return None;
    }

    if key == gtk::gdk::Key::Left || key == gtk::gdk::Key::Up {
        Some(gtk::DirectionType::TabBackward)
    } else if key == gtk::gdk::Key::Right || key == gtk::gdk::Key::Down {
        Some(gtk::DirectionType::TabForward)
    } else {
        None
    }
}

fn pdf_file_dialog(title: &str, accept_label: &str, initial_name: Option<&str>) -> gtk::FileDialog {
    let dialog = gtk::FileDialog::builder()
        .title(title)
        .accept_label(accept_label)
        .modal(true)
        .filters(&pdf_filters())
        .build();

    if let Some(initial_name) = initial_name {
        dialog.set_initial_name(Some(initial_name));
    }

    dialog
}

fn format_page_range(start: u32, end: u32) -> String {
    if start == end {
        start.to_string()
    } else {
        format!("{start}-{end}")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DelayedEntryValidationState, EntryValidationDisplay, fit_size, format_page_ranges,
        output_pdf_name, tile_control_focus_direction,
    };
    use std::path::Path;

    #[test]
    fn format_page_ranges_groups_contiguous_pages() {
        assert_eq!(format_page_ranges(&[1, 2, 3, 5, 7, 8]), "1-3,5,7-8");
    }

    #[test]
    fn format_page_ranges_handles_empty_single_and_sparse_pages() {
        assert_eq!(format_page_ranges(&[]), "");
        assert_eq!(format_page_ranges(&[4]), "4");
        assert_eq!(format_page_ranges(&[1, 3, 5]), "1,3,5");
    }

    #[test]
    fn output_pdf_name_uses_input_stem_and_action() {
        assert_eq!(
            output_pdf_name(Path::new("/tmp/sample.pdf"), "organized"),
            "sample_organized.pdf"
        );
    }

    #[test]
    fn delayed_entry_validation_state_tracks_visible_error_level() {
        let state = DelayedEntryValidationState::default();

        assert_eq!(state.display(true), EntryValidationDisplay::Hidden);

        state.show_style();
        assert_eq!(state.display(true), EntryValidationDisplay::StyleOnly);
        assert_eq!(state.display(false), EntryValidationDisplay::Hidden);

        state.show_message();
        assert_eq!(state.display(true), EntryValidationDisplay::Message);

        state.reset();
        assert_eq!(state.display(true), EntryValidationDisplay::Hidden);
    }

    #[test]
    fn fit_size_preserves_aspect_ratio_for_portrait_and_landscape_previews() {
        assert_eq!(fit_size(200, 400, 200, 220), Some((110, 220)));
        assert_eq!(fit_size(400, 200, 200, 220), Some((200, 100)));
    }

    #[test]
    fn fit_size_allows_previews_to_fill_larger_slots() {
        assert_eq!(fit_size(100, 50, 200, 220), Some((200, 100)));
    }

    #[test]
    fn fit_size_rejects_invalid_dimensions() {
        assert_eq!(fit_size(0, 100, 200, 220), None);
        assert_eq!(fit_size(100, 100, -1, 220), None);
    }

    #[test]
    fn rotated_size_swaps_dimensions_for_quarter_turns() {
        assert_eq!(crate::image::rotated_size(100, 200, 90), (200, 100));
        assert_eq!(crate::image::rotated_size(100, 200, 270), (200, 100));
        assert_eq!(crate::image::rotated_size(100, 200, 180), (100, 200));
    }

    #[test]
    fn tile_control_arrows_follow_the_control_order() {
        let no_modifiers = gtk::gdk::ModifierType::empty();

        assert_eq!(
            tile_control_focus_direction(gtk::gdk::Key::Left, no_modifiers),
            Some(gtk::DirectionType::TabBackward)
        );
        assert_eq!(
            tile_control_focus_direction(gtk::gdk::Key::Up, no_modifiers),
            Some(gtk::DirectionType::TabBackward)
        );
        assert_eq!(
            tile_control_focus_direction(gtk::gdk::Key::Right, no_modifiers),
            Some(gtk::DirectionType::TabForward)
        );
        assert_eq!(
            tile_control_focus_direction(gtk::gdk::Key::Down, no_modifiers),
            Some(gtk::DirectionType::TabForward)
        );
        assert_eq!(
            tile_control_focus_direction(
                gtk::gdk::Key::Right,
                gtk::gdk::ModifierType::CONTROL_MASK
            ),
            None
        );
    }
}
