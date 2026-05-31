use adw::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::gdk_pixbuf::{InterpType, Pixbuf, PixbufRotation};
use gtk::{gio, glib};
use std::io::Cursor;
use std::path::{Path, PathBuf};

const LIST_COLLECTION_PREVIEW_WIDTH: i32 = 128;
const LIST_COLLECTION_PREVIEW_HEIGHT: i32 = 128;
const GRID_COLLECTION_PREVIEW_WIDTH: i32 = 200;
const GRID_COLLECTION_PREVIEW_HEIGHT: i32 = 220;

pub(super) fn pdf_filters() -> gio::ListStore {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&gettext("PDF Documents")));
    filter.add_mime_type("application/pdf");
    filter.add_pattern("*.pdf");

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
    let builder = gtk::Builder::from_resource("/com/fvtronics/folios/password-dialog.ui");
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

pub(super) fn set_entry_validation_error(entry: &adw::EntryRow, has_error: bool, message: &str) {
    if has_error {
        entry.add_css_class("error");
        entry.set_tooltip_text(Some(message));
    } else {
        entry.remove_css_class("error");
        entry.set_tooltip_text(None);
    }
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
        .activatable(true)
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
    match rotated_preview_pixbuf(preview, rotation).map(|pixbuf| pixbuf_picture(&pixbuf)) {
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
        picture.set_size_request(180, 248);
        picture.upcast()
    } else {
        let placeholder = gtk::Image::from_icon_name("view-paged-symbolic");
        placeholder.set_size_request(180, 248);
        placeholder.upcast()
    }
}

pub(super) fn preview_tile() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .width_request(220)
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
        let picture = match rotated_preview_pixbuf(preview, rotation)
            .and_then(|pixbuf| fit_pixbuf(&pixbuf, width, height))
            .map(|pixbuf| pixbuf_picture(&pixbuf))
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
        .and_then(|preview| {
            rotated_preview_pixbuf(preview, rotation)
                .and_then(|pixbuf| fit_size(pixbuf.width(), pixbuf.height(), width, height))
        })
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
    slot.set_size_request(width, height);
    slot.set_halign(gtk::Align::Center);
    slot.set_valign(gtk::Align::Center);
    slot.set_child(Some(child));
    slot.upcast()
}

fn collection_preview_placeholder(width: i32, height: i32) -> gtk::Widget {
    let placeholder = gtk::Image::from_icon_name("view-paged-symbolic");
    placeholder.set_pixel_size((width.min(height) / 2).max(16));
    placeholder.set_size_request(width, height);
    placeholder.upcast()
}

fn blank_page_texture(width: i32, height: i32) -> Option<gtk::gdk::Texture> {
    let png_data = blank_page_png(width, height)?;
    gtk::gdk::Texture::from_bytes(&glib::Bytes::from(&png_data)).ok()
}

fn blank_page_png(width: i32, height: i32) -> Option<Vec<u8>> {
    let surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, width.max(1), height.max(1)).ok()?;
    let context = cairo::Context::new(&surface).ok()?;

    context.set_source_rgb(1.0, 1.0, 1.0);
    context.paint().ok()?;
    context.set_source_rgba(0.0, 0.0, 0.0, 0.25);
    context.set_line_width(1.0);
    context.rectangle(0.5, 0.5, width as f64 - 1.0, height as f64 - 1.0);
    context.stroke().ok()?;
    surface.flush();

    let mut png_data = Vec::new();
    surface.write_to_png(&mut png_data).ok()?;
    Some(png_data)
}

fn fit_pixbuf(pixbuf: &Pixbuf, max_width: i32, max_height: i32) -> Option<Pixbuf> {
    let (width, height) = fit_size(pixbuf.width(), pixbuf.height(), max_width, max_height)?;
    if width == pixbuf.width() && height == pixbuf.height() {
        Some(pixbuf.clone())
    } else {
        pixbuf.scale_simple(width, height, InterpType::Bilinear)
    }
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
    let (page_width, page_height) = rotated_size(page_width, page_height, rotation);
    fit_size(page_width, page_height, width, height).unwrap_or((width, height))
}

fn rotated_size(width: i32, height: i32, rotation: i64) -> (i32, i32) {
    if matches!(normalize_rotation(rotation), 90 | 270) {
        (height, width)
    } else {
        (width, height)
    }
}

fn rotated_preview_pixbuf(preview: &crate::preview::PagePreview, rotation: i64) -> Option<Pixbuf> {
    let mut pixbuf = Pixbuf::from_read(Cursor::new(preview.png_data.clone())).ok()?;
    if let Some(rotated) = match normalize_rotation(rotation) {
        90 => pixbuf.rotate_simple(PixbufRotation::Clockwise),
        180 => pixbuf.rotate_simple(PixbufRotation::Upsidedown),
        270 => pixbuf.rotate_simple(PixbufRotation::Counterclockwise),
        _ => None,
    } {
        pixbuf = rotated;
    }
    Some(pixbuf)
}

fn pixbuf_picture(pixbuf: &Pixbuf) -> gtk::Picture {
    let format = if pixbuf.has_alpha() {
        gtk::gdk::MemoryFormat::R8g8b8a8
    } else {
        gtk::gdk::MemoryFormat::R8g8b8
    };
    let texture = gtk::gdk::MemoryTexture::new(
        pixbuf.width(),
        pixbuf.height(),
        format,
        &pixbuf.read_pixel_bytes(),
        pixbuf.rowstride() as usize,
    );
    let picture = gtk::Picture::for_paintable(&texture);
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
    gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build()
}

pub(super) fn clear_box(box_: &gtk::Box) {
    while let Some(child) = box_.first_child() {
        box_.remove(&child);
    }
}

pub(super) fn normalize_pages(mut pages: Vec<u32>) -> Vec<u32> {
    pages.sort_unstable();
    pages.dedup();
    pages
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

fn normalize_rotation(rotation: i64) -> i64 {
    rotation.rem_euclid(360)
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
    use super::{fit_size, format_page_ranges, normalize_pages, rotated_size};

    #[test]
    fn normalize_pages_sorts_and_removes_duplicates() {
        assert_eq!(normalize_pages(vec![3, 1, 3, 2]), vec![1, 2, 3]);
    }

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
        assert_eq!(rotated_size(100, 200, 90), (200, 100));
        assert_eq!(rotated_size(100, 200, 270), (200, 100));
        assert_eq!(rotated_size(100, 200, 180), (100, 200));
    }
}
