/* preview.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use gtk::{gio, prelude::*};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct PagePreview {
    pub page_number: u32,
    pub png_data: Vec<u8>,
}

#[derive(Debug)]
pub enum PreviewError {
    Load(String),
    Render(String),
    WorkerStopped,
}

impl fmt::Display for PreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(message) => write!(f, "Could not preview this PDF: {message}"),
            Self::Render(message) => write!(f, "Could not render page previews: {message}"),
            Self::WorkerStopped => write!(f, "The preview operation stopped unexpectedly."),
        }
    }
}

pub async fn render_page_previews(input_file: PathBuf) -> Result<Vec<PagePreview>, PreviewError> {
    gio::spawn_blocking(move || render_page_previews_blocking(input_file))
        .await
        .unwrap_or(Err(PreviewError::WorkerStopped))
}

pub async fn render_first_page_preview(
    input_file: PathBuf,
) -> Result<Option<PagePreview>, PreviewError> {
    gio::spawn_blocking(move || render_first_page_preview_blocking(input_file))
        .await
        .unwrap_or(Err(PreviewError::WorkerStopped))
}

fn render_page_previews_blocking(input_file: PathBuf) -> Result<Vec<PagePreview>, PreviewError> {
    let file = gio::File::for_path(input_file);
    let document = poppler::Document::from_file(file.uri().as_str(), None)
        .map_err(|error| PreviewError::Load(error.to_string()))?;
    let mut previews = Vec::with_capacity(document.n_pages() as usize);

    for index in 0..document.n_pages() {
        if let Some(preview) = render_page_preview(&document, index)? {
            previews.push(preview);
        }
    }

    Ok(previews)
}

fn render_first_page_preview_blocking(
    input_file: PathBuf,
) -> Result<Option<PagePreview>, PreviewError> {
    let file = gio::File::for_path(input_file);
    let document = poppler::Document::from_file(file.uri().as_str(), None)
        .map_err(|error| PreviewError::Load(error.to_string()))?;

    render_page_preview(&document, 0)
}

fn render_page_preview(
    document: &poppler::Document,
    index: i32,
) -> Result<Option<PagePreview>, PreviewError> {
    const PREVIEW_WIDTH: i32 = 160;

    let Some(page) = document.page(index) else {
        return Ok(None);
    };
    let (page_width, page_height) = page.size();
    if page_width <= 0.0 || page_height <= 0.0 {
        return Ok(None);
    }

    let scale = PREVIEW_WIDTH as f64 / page_width;
    let preview_height = (page_height * scale).ceil() as i32;
    let surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, PREVIEW_WIDTH, preview_height.max(1))
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
