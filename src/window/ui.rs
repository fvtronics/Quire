use adw::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::gdk_pixbuf::PixbufRotation;
use gtk::gio;
use std::io::Cursor;
use std::path::Path;

pub(super) fn pdf_filters() -> gio::ListStore {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&gettext("PDF Documents")));
    filter.add_mime_type("application/pdf");
    filter.add_pattern("*.pdf");

    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    filters
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

            let texture = gtk::gdk::Texture::for_pixbuf(&pixbuf);
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

fn format_page_range(start: u32, end: u32) -> String {
    if start == end {
        start.to_string()
    } else {
        format!("{start}-{end}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
