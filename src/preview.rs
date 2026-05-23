/* preview.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use gtk::{gio, glib, prelude::FileExt};
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

const PAGE_PREVIEW_WIDTH: i32 = 160;
const SINGLE_FILE_PREVIEW_WIDTH: i32 = 360;

#[derive(Debug, Clone)]
pub struct PagePreview {
    pub page_number: u32,
    pub png_data: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct DocumentPreviews {
    pub page_count: usize,
    pub previews: BTreeMap<u32, PagePreview>,
}

#[derive(Debug)]
pub enum PreviewError {
    PasswordRequired,
    InvalidPassword,
    Load(String),
    Render(String),
    WorkerStopped,
}

impl fmt::Display for PreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PasswordRequired => write!(f, "This PDF is password protected."),
            Self::InvalidPassword => write!(f, "The PDF password is incorrect."),
            Self::Load(message) => write!(f, "Could not preview this PDF: {message}"),
            Self::Render(message) => write!(f, "Could not render page previews: {message}"),
            Self::WorkerStopped => write!(f, "The preview operation stopped unexpectedly."),
        }
    }
}

pub async fn render_page_previews(
    input_file: PathBuf,
    password: Option<String>,
) -> Result<DocumentPreviews, PreviewError> {
    gio::spawn_blocking(move || render_page_previews_blocking(input_file, password))
        .await
        .unwrap_or(Err(PreviewError::WorkerStopped))
}

pub async fn render_first_page_preview(
    input_file: PathBuf,
    password: Option<String>,
) -> Result<Option<PagePreview>, PreviewError> {
    gio::spawn_blocking(move || render_first_page_preview_blocking(input_file, password))
        .await
        .unwrap_or(Err(PreviewError::WorkerStopped))
}

pub async fn render_single_file_preview(
    input_file: PathBuf,
    password: Option<String>,
) -> Result<Option<PagePreview>, PreviewError> {
    gio::spawn_blocking(move || render_single_file_preview_blocking(input_file, password))
        .await
        .unwrap_or(Err(PreviewError::WorkerStopped))
}

pub async fn render_single_file_preview_with_metadata(
    input_file: PathBuf,
    password: Option<String>,
) -> Result<(Option<PagePreview>, crate::pdf::PdfDocumentMetadata), PreviewError> {
    gio::spawn_blocking(move || {
        render_single_file_preview_with_metadata_blocking(input_file, password)
    })
    .await
    .unwrap_or(Err(PreviewError::WorkerStopped))
}

pub async fn render_first_page_preview_with_count(
    input_file: PathBuf,
    password: Option<String>,
) -> Result<(Option<PagePreview>, usize), PreviewError> {
    gio::spawn_blocking(move || render_first_page_preview_with_count_blocking(input_file, password))
        .await
        .unwrap_or(Err(PreviewError::WorkerStopped))
}

fn render_page_previews_blocking(
    input_file: PathBuf,
    password: Option<String>,
) -> Result<DocumentPreviews, PreviewError> {
    render_with_document(input_file, password, |document| {
        let page_count = document.n_pages().max(0) as usize;
        let mut previews = BTreeMap::new();

        for index in 0..page_count as i32 {
            if let Some(preview) = render_page_preview(document, index, PAGE_PREVIEW_WIDTH)? {
                previews.insert(preview.page_number, preview);
            }
        }

        Ok(DocumentPreviews {
            page_count,
            previews,
        })
    })
}

fn render_first_page_preview_blocking(
    input_file: PathBuf,
    password: Option<String>,
) -> Result<Option<PagePreview>, PreviewError> {
    render_with_document(input_file, password, |document| {
        render_page_preview(document, 0, PAGE_PREVIEW_WIDTH)
    })
}

fn render_single_file_preview_blocking(
    input_file: PathBuf,
    password: Option<String>,
) -> Result<Option<PagePreview>, PreviewError> {
    render_with_document(input_file, password, |document| {
        render_page_preview(document, 0, SINGLE_FILE_PREVIEW_WIDTH)
    })
}

fn render_single_file_preview_with_metadata_blocking(
    input_file: PathBuf,
    password: Option<String>,
) -> Result<(Option<PagePreview>, crate::pdf::PdfDocumentMetadata), PreviewError> {
    render_with_document(input_file, password, |document| {
        Ok((
            render_page_preview(document, 0, SINGLE_FILE_PREVIEW_WIDTH)?,
            document_metadata(document),
        ))
    })
}

fn render_first_page_preview_with_count_blocking(
    input_file: PathBuf,
    password: Option<String>,
) -> Result<(Option<PagePreview>, usize), PreviewError> {
    render_with_document(input_file, password, |document| {
        let page_count = document.n_pages().max(0) as usize;

        Ok((
            render_page_preview(document, 0, SINGLE_FILE_PREVIEW_WIDTH)?,
            page_count,
        ))
    })
}

fn render_with_document<T, Render>(
    input_file: PathBuf,
    password: Option<String>,
    render: Render,
) -> Result<T, PreviewError>
where
    Render: FnOnce(&poppler::Document) -> Result<T, PreviewError>,
{
    let document = load_document(input_file, password.as_deref())?;
    render(&document)
}

fn load_document(
    input_file: PathBuf,
    password: Option<&str>,
) -> Result<poppler::Document, PreviewError> {
    let file = gio::File::for_path(input_file);
    poppler::Document::from_file(file.uri().as_str(), password)
        .map_err(|error| document_load_error(error, password.is_some()))
}

fn document_load_error(error: glib::Error, had_password: bool) -> PreviewError {
    if error.matches(poppler::Error::Encrypted) {
        if had_password {
            PreviewError::InvalidPassword
        } else {
            PreviewError::PasswordRequired
        }
    } else {
        PreviewError::Load(error.to_string())
    }
}

fn render_page_preview(
    document: &poppler::Document,
    index: i32,
    preview_width: i32,
) -> Result<Option<PagePreview>, PreviewError> {
    let Some(page) = document.page(index) else {
        return Ok(None);
    };

    if let Some(preview) = embedded_page_thumbnail(&page, index as u32 + 1, preview_width) {
        return Ok(Some(preview));
    }

    let (page_width, page_height) = page.size();
    if page_width <= 0.0 || page_height <= 0.0 {
        return Ok(None);
    }

    let scale = preview_width as f64 / page_width;
    let preview_height = (page_height * scale).ceil() as i32;
    let surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, preview_width, preview_height.max(1))
            .map_err(|error| PreviewError::Render(error.to_string()))?;
    let context =
        cairo::Context::new(&surface).map_err(|error| PreviewError::Render(error.to_string()))?;

    context.set_source_rgb(1.0, 1.0, 1.0);
    context
        .paint()
        .map_err(|error| PreviewError::Render(error.to_string()))?;
    context.scale(scale, scale);
    page.render(&context);
    surface.flush();

    let mut png_data = Vec::new();
    surface
        .write_to_png(&mut png_data)
        .map_err(|error| PreviewError::Render(error.to_string()))?;

    Ok(Some(PagePreview {
        page_number: index as u32 + 1,
        png_data,
    }))
}

fn document_metadata(document: &poppler::Document) -> crate::pdf::PdfDocumentMetadata {
    crate::pdf::PdfDocumentMetadata {
        title: document.title().map(String::from).unwrap_or_default(),
        author: document.author().map(String::from).unwrap_or_default(),
        subject: document.subject().map(String::from).unwrap_or_default(),
        keywords: document.keywords().map(String::from).unwrap_or_default(),
        creator: document.creator().map(String::from).unwrap_or_default(),
        producer: document.producer().map(String::from).unwrap_or_default(),
    }
}

fn embedded_page_thumbnail(
    page: &poppler::Page,
    page_number: u32,
    minimum_width: i32,
) -> Option<PagePreview> {
    let (width, _) = page.thumbnail_size()?;
    if width < minimum_width {
        return None;
    }

    let mut png_data = Vec::new();
    page.thumbnail()?.write_to_png(&mut png_data).ok()?;

    Some(PagePreview {
        page_number,
        png_data,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        render_first_page_preview_blocking, render_first_page_preview_with_count_blocking,
        render_page_previews_blocking, PreviewError,
    };
    use lopdf::{
        dictionary, Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream,
    };
    use std::path::Path;

    #[test]
    fn render_page_previews_returns_page_count_and_pngs() {
        let dir = tempfile::tempdir().expect("test directory should be created");
        let input = dir.path().join("input.pdf");
        write_test_pdf(&input, 3);

        let previews = render_page_previews_blocking(input, None).unwrap();

        assert_eq!(previews.page_count, 3);
        assert_eq!(
            previews.previews.keys().copied().collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        for preview in previews.previews.values() {
            assert_png(&preview.png_data);
        }
    }

    #[test]
    fn render_first_page_preview_with_count_keeps_document_page_count() {
        let dir = tempfile::tempdir().expect("test directory should be created");
        let input = dir.path().join("input.pdf");
        write_test_pdf(&input, 2);

        let (preview, page_count) =
            render_first_page_preview_with_count_blocking(input, None).unwrap();

        assert_eq!(page_count, 2);
        let preview = preview.expect("first page preview should render");
        assert_eq!(preview.page_number, 1);
        assert_png(&preview.png_data);
    }

    #[test]
    fn render_first_page_preview_reports_password_errors() {
        let dir = tempfile::tempdir().expect("test directory should be created");
        let input = dir.path().join("locked.pdf");
        write_encrypted_test_pdf(&input, "secret");

        assert!(matches!(
            render_first_page_preview_blocking(input.clone(), None),
            Err(PreviewError::PasswordRequired)
        ));
        assert!(matches!(
            render_first_page_preview_blocking(input.clone(), Some("wrong".to_string())),
            Err(PreviewError::InvalidPassword)
        ));
        assert!(
            render_first_page_preview_blocking(input, Some("secret".to_string()))
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn preview_error_messages_are_actionable() {
        assert_eq!(
            PreviewError::PasswordRequired.to_string(),
            "This PDF is password protected."
        );
        assert_eq!(
            PreviewError::InvalidPassword.to_string(),
            "The PDF password is incorrect."
        );
        assert_eq!(
            PreviewError::WorkerStopped.to_string(),
            "The preview operation stopped unexpectedly."
        );
    }

    fn write_test_pdf(path: &Path, page_count: usize) {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let mut kids = Vec::with_capacity(page_count);

        for _ in 0..page_count {
            let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
            let page_id = document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "Resources" => dictionary! {},
                "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            });
            kids.push(page_id.into());
        }

        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => page_count as i64,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document.trailer.set(
            "ID",
            Object::Array(vec![
                Object::string_literal(b"folios-test-id-1"),
                Object::string_literal(b"folios-test-id-2"),
            ]),
        );
        document.compress();
        document.save(path).expect("test PDF should be saved");
    }

    fn write_encrypted_test_pdf(path: &Path, password: &str) {
        write_test_pdf(path, 1);

        let mut document = Document::load(path).expect("test PDF should load");
        let version = EncryptionVersion::V1 {
            document: &document,
            owner_password: password,
            user_password: password,
            permissions: Permissions::PRINTABLE,
        };
        let state = EncryptionState::try_from(version).expect("encryption state should build");
        document.encrypt(&state).expect("test PDF should encrypt");
        document
            .save(path)
            .expect("encrypted test PDF should be saved");
    }

    fn assert_png(data: &[u8]) {
        assert!(
            data.starts_with(b"\x89PNG\r\n\x1a\n"),
            "preview should be PNG data"
        );
    }
}
