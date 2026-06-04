/* pdf.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

mod document;
mod metadata;
mod operations;
mod pages;
mod ranges;
mod types;

pub use operations::{
    compress_pdf, edit_pdf_metadata, extract_pages, merge_pdfs, organize_pdf, split_pdf,
    watermark_pdf,
};
pub(crate) use ranges::SPLIT_PAGE_COUNT_HINT;
pub use ranges::{parse_page_numbers, parse_page_ranges};
pub use types::{
    CompressOptions, PageSelection, PdfBackendError, PdfDocumentMetadata, PdfEditableMetadata,
    PdfInput, PdfOutputOptions, PdfSaveOptions, SplitRule, WatermarkLayer, WatermarkOptions,
    WatermarkTarget,
};

#[cfg(test)]
use document::load_document;
#[cfg(test)]
use metadata::{app_producer_metadata, edit_pdf_metadata_blocking};
#[cfg(test)]
use operations::{
    compress_pdf_blocking, merge_pdfs_blocking, split_pdf_blocking, watermark_pdf_blocking,
    write_selected_pages,
};
#[cfg(test)]
use ranges::split_breaks;

#[cfg(test)]
mod tests;
