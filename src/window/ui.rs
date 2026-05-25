use adw::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::gdk_pixbuf::PixbufRotation;
use gtk::{gio, glib};
use std::io::Cursor;
use std::path::{Path, PathBuf};

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
        .activatable(false)
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
    let picture = match gtk::gdk_pixbuf::Pixbuf::from_read(Cursor::new(preview.png_data.clone())) {
        Ok(mut pixbuf) => {
            if let Some(rotated) = match normalize_rotation(rotation) {
                90 => pixbuf.rotate_simple(PixbufRotation::Clockwise),
                180 => pixbuf.rotate_simple(PixbufRotation::Upsidedown),
                270 => pixbuf.rotate_simple(PixbufRotation::Counterclockwise),
                _ => None,
            } {
                pixbuf = rotated;
            }

            let texture = gtk::gdk::Texture::from_bytes(&glib::Bytes::from(
                &pixbuf.save_to_bufferv("png", &[]).unwrap(),
            ))
            .unwrap();
            gtk::Picture::for_paintable(&texture)
        }
        Err(_) => gtk::Picture::new(),
    };
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture
}

pub(super) fn rotated_list_preview_prefix(
    preview: Option<&crate::preview::PagePreview>,
    rotation: i64,
) -> gtk::Widget {
    if let Some(preview) = preview {
        let picture = rotated_preview_picture(preview, rotation);
        picture.set_size_request(48, 68);
        picture.upcast()
    } else {
        gtk::Image::from_icon_name("view-paged-symbolic").upcast()
    }
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
        .width_request(180)
        .build()
}

pub(super) fn tile_preview_widget(
    preview: Option<&crate::preview::PagePreview>,
    rotation: i64,
) -> gtk::Widget {
    if let Some(preview) = preview {
        let picture = rotated_preview_picture(preview, rotation);
        picture.set_size_request(160, 220);
        picture.upcast()
    } else {
        let placeholder = gtk::Image::from_icon_name("view-paged-symbolic");
        placeholder.set_size_request(160, 220);
        placeholder.upcast()
    }
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
    use super::{format_page_ranges, normalize_pages};

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
}
