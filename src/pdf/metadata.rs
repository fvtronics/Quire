/* metadata.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use lopdf::{dictionary, text_string, Dictionary, Document, IncrementalDocument, Object};
use std::path::PathBuf;

use super::document::{load_document, remove_metadata, save_document, temporary_output_file};
use super::types::{PdfBackendError, PdfEditableMetadata, PdfSaveOptions};

pub(super) fn edit_pdf_metadata_blocking(
    input_file: PathBuf,
    password: Option<String>,
    output_file: PathBuf,
    metadata: PdfEditableMetadata,
    options: PdfSaveOptions,
) -> Result<PathBuf, PdfBackendError> {
    if input_file == output_file {
        return Err(PdfBackendError::OutputMatchesInput);
    }

    if !options.remove_metadata && !options.modern_pdf && password.is_none() {
        return edit_pdf_metadata_incremental(input_file, output_file, metadata);
    }

    let mut document = load_document(&input_file, password.as_deref())?;
    let catalog_id = document
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?;

    if options.remove_metadata {
        remove_metadata(&mut document);
    }
    apply_document_metadata(&mut document, metadata);
    save_document(
        document,
        catalog_id,
        &output_file,
        PdfSaveOptions {
            remove_metadata: false,
            modern_pdf: options.modern_pdf,
        },
    )
}

fn edit_pdf_metadata_incremental(
    input_file: PathBuf,
    output_file: PathBuf,
    metadata: PdfEditableMetadata,
) -> Result<PathBuf, PdfBackendError> {
    let mut document =
        IncrementalDocument::load(&input_file).map_err(|error| PdfBackendError::Load {
            path: input_file.clone(),
            message: error.to_string(),
        })?;

    if let Some(info_id) = document
        .get_prev_documents()
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|object| object.as_reference().ok())
    {
        document
            .opt_clone_object_to_new_document(info_id)
            .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?;
    }

    apply_document_metadata(&mut document.new_document, metadata);
    set_app_producer_metadata(&mut document.new_document);

    let temp_file = temporary_output_file(&output_file)?;
    document
        .save(temp_file.path())
        .map_err(|error| PdfBackendError::Write(error.to_string()))?;
    std::fs::copy(temp_file.path(), &output_file).map_err(PdfBackendError::Save)?;
    Ok(output_file)
}

fn apply_document_metadata(document: &mut Document, metadata: PdfEditableMetadata) {
    let info_id = document
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|object| object.as_reference().ok());
    let mut dictionary = document
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|object| document.dereference(object).ok())
        .and_then(|(_, object)| object.as_dict().ok())
        .cloned()
        .unwrap_or_default();

    set_metadata_field(&mut dictionary, b"Title", &metadata.title);
    set_metadata_field(&mut dictionary, b"Author", &metadata.author);
    set_metadata_field(&mut dictionary, b"Subject", &metadata.subject);
    set_metadata_field(&mut dictionary, b"Keywords", &metadata.keywords);

    if dictionary.is_empty() {
        document.trailer.remove(b"Info");
        if let Some(info_id) = info_id {
            document.delete_object(info_id);
        }
    } else if let Some(info_id) = info_id {
        document
            .objects
            .insert(info_id, Object::Dictionary(dictionary));
    } else {
        let info_id = document.add_object(dictionary);
        document.trailer.set("Info", info_id);
    }
}

fn set_metadata_field(dictionary: &mut Dictionary, key: &[u8], value: &str) {
    let value = value.trim();
    if value.is_empty() {
        dictionary.remove(key);
    } else {
        dictionary.set(key, text_string(value));
    }
}

pub(super) fn set_app_producer_metadata(document: &mut Document) {
    let producer = app_producer_metadata();
    let info_id = document
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|object| object.as_reference().ok());

    if let Some(info_id) = info_id
        && let Ok(dictionary) = document
            .get_object_mut(info_id)
            .and_then(Object::as_dict_mut)
    {
        dictionary.set("Producer", text_string(&producer));
        return;
    }

    let info_id = document.add_object(dictionary! {
        "Producer" => text_string(&producer),
    });
    document.trailer.set("Info", info_id);
}

pub(super) fn app_producer_metadata() -> String {
    format!("Quire {}", crate::config::VERSION)
}
