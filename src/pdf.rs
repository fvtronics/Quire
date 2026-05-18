/* pdf.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use gtk::gio;
use lopdf::{Document, Object, ObjectId};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum PdfBackendError {
    NotEnoughInputs,
    OutputMatchesInput,
    Load { path: PathBuf, message: String },
    InvalidDocument(String),
    Write(String),
    Save(std::io::Error),
    WorkerStopped,
}

impl fmt::Display for PdfBackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEnoughInputs => write!(f, "Choose at least two PDF files to merge."),
            Self::OutputMatchesInput => {
                write!(
                    f,
                    "Save the merged PDF as a new file, not over an input file."
                )
            }
            Self::Load { path, message } => write!(
                f,
                "Could not read {}: {message}",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("PDF")
            ),
            Self::InvalidDocument(message) => write!(f, "Could not merge these PDFs: {message}"),
            Self::Write(message) => write!(f, "Could not write the merged PDF: {message}"),
            Self::Save(error) => write!(f, "Could not save the merged PDF: {error}"),
            Self::WorkerStopped => write!(f, "The merge operation stopped unexpectedly."),
        }
    }
}

pub async fn merge_pdfs(
    input_files: Vec<PathBuf>,
    output_file: PathBuf,
) -> Result<PathBuf, PdfBackendError> {
    gio::spawn_blocking(move || merge_pdfs_blocking(input_files, output_file))
        .await
        .unwrap_or(Err(PdfBackendError::WorkerStopped))
}

fn merge_pdfs_blocking(
    input_files: Vec<PathBuf>,
    output_file: PathBuf,
) -> Result<PathBuf, PdfBackendError> {
    if input_files.len() < 2 {
        return Err(PdfBackendError::NotEnoughInputs);
    }

    if input_files.iter().any(|path| path == &output_file) {
        return Err(PdfBackendError::OutputMatchesInput);
    }

    let temp_file = temporary_output_path(&output_file);
    let mut max_id = 1;
    let mut page_objects = BTreeMap::new();
    let mut document_objects = BTreeMap::new();
    let mut merged = Document::with_version("1.5");

    for path in input_files {
        let mut document = Document::load(&path).map_err(|error| PdfBackendError::Load {
            path: path.clone(),
            message: error.to_string(),
        })?;

        document.renumber_objects_with(max_id);
        max_id = document.max_id + 1;

        for object_id in document.get_pages().into_values() {
            let object = document
                .get_object(object_id)
                .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?
                .to_owned();
            page_objects.insert(object_id, object);
        }

        document_objects.extend(document.objects);
    }

    let (catalog_id, catalog_object, pages_id, pages_object) =
        collect_document_roots(document_objects, &mut merged)?;

    for (object_id, object) in page_objects.iter() {
        let mut dictionary = object
            .as_dict()
            .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?
            .clone();
        dictionary.set("Parent", pages_id);
        merged
            .objects
            .insert(*object_id, Object::Dictionary(dictionary));
    }

    let mut pages_dictionary = pages_object
        .as_dict()
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?
        .clone();
    pages_dictionary.set("Count", page_objects.len() as u32);
    pages_dictionary.set(
        "Kids",
        page_objects
            .into_keys()
            .map(Object::Reference)
            .collect::<Vec<_>>(),
    );
    merged
        .objects
        .insert(pages_id, Object::Dictionary(pages_dictionary));

    let mut catalog_dictionary = catalog_object
        .as_dict()
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?
        .clone();
    catalog_dictionary.set("Pages", pages_id);
    catalog_dictionary.remove(b"Outlines");
    merged
        .objects
        .insert(catalog_id, Object::Dictionary(catalog_dictionary));

    merged.trailer.set("Root", catalog_id);
    merged.max_id = merged.objects.len() as u32;
    merged.renumber_objects();
    merged.adjust_zero_pages();

    merged
        .save(&temp_file)
        .map_err(|error| PdfBackendError::Write(error.to_string()))?;

    std::fs::rename(&temp_file, &output_file).map_err(PdfBackendError::Save)?;
    Ok(output_file)
}

fn collect_document_roots(
    document_objects: BTreeMap<ObjectId, Object>,
    merged: &mut Document,
) -> Result<(ObjectId, Object, ObjectId, Object), PdfBackendError> {
    let mut catalog_object: Option<(ObjectId, Object)> = None;
    let mut pages_object: Option<(ObjectId, Object)> = None;

    for (object_id, object) in document_objects {
        match object.type_name().unwrap_or(b"") {
            b"Catalog" => {
                catalog_object = Some((
                    catalog_object
                        .as_ref()
                        .map(|(id, _): &(ObjectId, Object)| *id)
                        .unwrap_or(object_id),
                    object,
                ));
            }
            b"Pages" => {
                let dictionary = object
                    .as_dict()
                    .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?;
                let mut dictionary = dictionary.clone();
                if let Some((_, previous)) = pages_object.as_ref() {
                    if let Ok(previous_dictionary) = previous.as_dict() {
                        dictionary.extend(previous_dictionary);
                    }
                }

                pages_object = Some((
                    pages_object
                        .as_ref()
                        .map(|(id, _): &(ObjectId, Object)| *id)
                        .unwrap_or(object_id),
                    Object::Dictionary(dictionary),
                ));
            }
            b"Page" | b"Outlines" | b"Outline" => {}
            _ => {
                merged.objects.insert(object_id, object);
            }
        }
    }

    let (catalog_id, catalog_object) = catalog_object
        .ok_or_else(|| PdfBackendError::InvalidDocument("catalog root not found".to_string()))?;
    let (pages_id, pages_object) = pages_object
        .ok_or_else(|| PdfBackendError::InvalidDocument("pages root not found".to_string()))?;

    Ok((catalog_id, catalog_object, pages_id, pages_object))
}

fn temporary_output_path(output_file: &Path) -> PathBuf {
    let directory = output_file.parent().unwrap_or_else(|| Path::new("."));
    let name = output_file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("merged");

    directory.join(format!(".{name}.folios-{}.pdf", std::process::id()))
}
