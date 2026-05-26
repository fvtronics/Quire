/* operations.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use gtk::gio;
use lopdf::{Document, Object};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::document::{
    build_and_save_document, load_document, remove_metadata, save_document, OutputPages,
};
use super::metadata::edit_pdf_metadata_blocking;
use super::pages::{blank_page_object, page_selections, rotated_page_object};
use super::ranges::{split_breaks, split_output_prefix};
use super::types::{
    CompressOptions, PageSelection, PdfBackendError, PdfEditableMetadata, PdfInput,
    PdfOutputOptions, PdfSaveOptions, SplitRule,
};

pub async fn merge_pdfs(
    input_files: Vec<PdfInput>,
    output_file: PathBuf,
    options: PdfOutputOptions,
) -> Result<PathBuf, PdfBackendError> {
    gio::spawn_blocking(move || merge_pdfs_blocking(input_files, output_file, options))
        .await
        .unwrap_or(Err(PdfBackendError::WorkerStopped))
}

pub async fn organize_pdf(
    input_file: PathBuf,
    password: Option<String>,
    page_order: Vec<PageSelection>,
    output_file: PathBuf,
    options: PdfOutputOptions,
) -> Result<PathBuf, PdfBackendError> {
    gio::spawn_blocking(move || {
        write_selected_pages(input_file, password, page_order, output_file, options)
    })
    .await
    .unwrap_or(Err(PdfBackendError::WorkerStopped))
}

pub async fn extract_pages(
    input_file: PathBuf,
    password: Option<String>,
    pages: Vec<PageSelection>,
    output_file: PathBuf,
    options: PdfOutputOptions,
) -> Result<PathBuf, PdfBackendError> {
    gio::spawn_blocking(move || {
        write_selected_pages(input_file, password, pages, output_file, options)
    })
    .await
    .unwrap_or(Err(PdfBackendError::WorkerStopped))
}

pub async fn split_pdf(
    input_file: PathBuf,
    password: Option<String>,
    output_folder: PathBuf,
    prefix: String,
    rule: SplitRule,
    options: PdfOutputOptions,
) -> Result<PathBuf, PdfBackendError> {
    gio::spawn_blocking(move || {
        split_pdf_blocking(input_file, password, output_folder, prefix, rule, options)
    })
    .await
    .unwrap_or(Err(PdfBackendError::WorkerStopped))
}

pub async fn compress_pdf(
    input_file: PathBuf,
    password: Option<String>,
    output_file: PathBuf,
    options: CompressOptions,
) -> Result<PathBuf, PdfBackendError> {
    gio::spawn_blocking(move || compress_pdf_blocking(input_file, password, output_file, options))
        .await
        .unwrap_or(Err(PdfBackendError::WorkerStopped))
}

pub async fn edit_pdf_metadata(
    input_file: PathBuf,
    password: Option<String>,
    output_file: PathBuf,
    metadata: PdfEditableMetadata,
    options: PdfSaveOptions,
) -> Result<PathBuf, PdfBackendError> {
    gio::spawn_blocking(move || {
        edit_pdf_metadata_blocking(input_file, password, output_file, metadata, options)
    })
    .await
    .unwrap_or(Err(PdfBackendError::WorkerStopped))
}

pub(super) fn merge_pdfs_blocking(
    input_files: Vec<PdfInput>,
    output_file: PathBuf,
    options: PdfOutputOptions,
) -> Result<PathBuf, PdfBackendError> {
    if input_files.len() < 2 {
        return Err(PdfBackendError::NotEnoughInputs);
    }

    if input_files.iter().any(|input| input.path == output_file) {
        return Err(PdfBackendError::OutputMatchesInput);
    }

    let mut max_id = 1;
    let mut output_pages = OutputPages::new();
    let mut document_objects = BTreeMap::new();

    for input in input_files {
        let mut document = load_document(&input.path, input.password.as_deref())?;

        document.renumber_objects_with(max_id);
        max_id = document.max_id + 1;

        for object_id in document.get_pages().into_values() {
            output_pages.push(
                object_id,
                rotated_page_object(&document, object_id, input.rotation)?,
            );
        }

        document_objects.extend(document.objects);
    }

    build_and_save_document(document_objects, output_pages, &output_file, options)
}

pub(super) fn write_selected_pages(
    input_file: PathBuf,
    password: Option<String>,
    page_numbers: Vec<PageSelection>,
    output_file: PathBuf,
    options: PdfOutputOptions,
) -> Result<PathBuf, PdfBackendError> {
    let document = load_document(&input_file, password.as_deref())?;

    write_selected_pages_from_document(&input_file, &document, page_numbers, &output_file, options)
}

fn write_selected_pages_from_document(
    input_file: &Path,
    document: &Document,
    page_numbers: Vec<PageSelection>,
    output_file: &Path,
    options: PdfOutputOptions,
) -> Result<PathBuf, PdfBackendError> {
    if page_numbers.is_empty() {
        return Err(PdfBackendError::NoPagesSelected);
    }

    if input_file == output_file {
        return Err(PdfBackendError::OutputMatchesInput);
    }

    let pages = document.get_pages();
    let mut output_pages = OutputPages::with_capacity(page_numbers.len());
    let mut next_page_id = document.max_id + 1;

    for selection in page_numbers {
        let object_id = pages.get(&selection.page_number).ok_or_else(|| {
            PdfBackendError::InvalidPageRange(format!(
                "Page {} is not in this PDF.",
                selection.page_number
            ))
        })?;
        let page_id = (next_page_id, 0);
        next_page_id += 1;
        let page = if selection.is_blank() {
            blank_page_object(document, *object_id, selection.rotation)?
        } else {
            rotated_page_object(document, *object_id, selection.rotation)?
        };
        output_pages.push(page_id, page);
    }

    build_and_save_document(document.objects.clone(), output_pages, output_file, options)
}

pub(super) fn split_pdf_blocking(
    input_file: PathBuf,
    password: Option<String>,
    output_folder: PathBuf,
    prefix: String,
    rule: SplitRule,
    options: PdfOutputOptions,
) -> Result<PathBuf, PdfBackendError> {
    let document = load_document(&input_file, password.as_deref())?;
    let page_count = document.get_pages().len() as u32;
    if page_count == 0 {
        return Err(PdfBackendError::NoPagesSelected);
    }

    let prefix = split_output_prefix(&input_file, &prefix);
    let mut start = 1;
    let mut index = 1;
    for end in split_breaks(rule, page_count)? {
        if end < start {
            continue;
        }

        let output_file = output_folder.join(format!("{} {}.pdf", prefix, index));
        let page_numbers = page_selections(start..=end);
        write_selected_pages_from_document(
            &input_file,
            &document,
            page_numbers,
            &output_file,
            options,
        )?;
        start = end + 1;
        index += 1;
    }

    if start <= page_count {
        let output_file = output_folder.join(format!("{} {}.pdf", prefix, index));
        let page_numbers = page_selections(start..=page_count);
        write_selected_pages_from_document(
            &input_file,
            &document,
            page_numbers,
            &output_file,
            options,
        )?;
    }

    Ok(output_folder)
}

pub(super) fn compress_pdf_blocking(
    input_file: PathBuf,
    password: Option<String>,
    output_file: PathBuf,
    options: CompressOptions,
) -> Result<PathBuf, PdfBackendError> {
    if input_file == output_file {
        return Err(PdfBackendError::OutputMatchesInput);
    }

    let mut document = load_document(&input_file, password.as_deref())?;
    let catalog_id = document
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?;

    if options.remove_empty_streams {
        document.delete_zero_length_streams();
    }
    if options.prune_objects {
        document.prune_objects();
    }
    if options.save.remove_metadata {
        remove_metadata(&mut document);
    }
    document.compress();

    save_document(document, catalog_id, &output_file, options.save)
}
