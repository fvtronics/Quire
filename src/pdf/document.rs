/* document.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use lopdf::{Document, Object, ObjectId};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::metadata::set_app_producer_metadata;
use super::pages::normalize_page_sizes;
use super::types::{PdfBackendError, PdfOutputOptions, PdfSaveOptions};

pub(super) fn load_document(
    path: &Path,
    password: Option<&str>,
) -> Result<Document, PdfBackendError> {
    let document = match password {
        Some(password) => {
            Document::load_with_password(path, password).map_err(|error| match error {
                lopdf::Error::InvalidPassword => PdfBackendError::InvalidPassword {
                    path: path.to_path_buf(),
                },
                error => PdfBackendError::Load {
                    path: path.to_path_buf(),
                    message: error.to_string(),
                },
            })?
        }
        None => Document::load(path).map_err(|error| PdfBackendError::Load {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?,
    };

    if document.is_encrypted() {
        return Err(PdfBackendError::PasswordRequired {
            path: path.to_path_buf(),
        });
    }

    Ok(document)
}

pub(super) struct OutputPages {
    objects: BTreeMap<ObjectId, Object>,
    ordered_ids: Vec<ObjectId>,
}

impl OutputPages {
    pub(super) fn new() -> Self {
        Self {
            objects: BTreeMap::new(),
            ordered_ids: Vec::new(),
        }
    }

    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self {
            objects: BTreeMap::new(),
            ordered_ids: Vec::with_capacity(capacity),
        }
    }

    pub(super) fn push(&mut self, object_id: ObjectId, object: Object) {
        self.objects.insert(object_id, object);
        self.ordered_ids.push(object_id);
    }
}

pub(super) fn build_and_save_document(
    document_objects: BTreeMap<ObjectId, Object>,
    output_pages: OutputPages,
    output_file: &Path,
    options: PdfOutputOptions,
) -> Result<PathBuf, PdfBackendError> {
    let mut output = Document::with_version("1.5");
    let (catalog_id, catalog_object, pages_id, pages_object) =
        collect_document_roots(document_objects, &mut output)?;

    insert_output_pages(&mut output, pages_id, output_pages.objects)?;
    insert_pages_root(
        &mut output,
        pages_id,
        pages_object,
        &output_pages.ordered_ids,
    )?;
    insert_catalog_root(&mut output, catalog_id, catalog_object, pages_id)?;
    output.trailer.set("Root", catalog_id);
    apply_output_options(&mut output, &output_pages.ordered_ids, options)?;

    save_document(output, catalog_id, output_file, options.save)
}

fn insert_output_pages(
    document: &mut Document,
    pages_id: ObjectId,
    page_objects: BTreeMap<ObjectId, Object>,
) -> Result<(), PdfBackendError> {
    for (object_id, object) in page_objects {
        let mut dictionary = object
            .as_dict()
            .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?
            .clone();
        dictionary.set("Parent", pages_id);
        document
            .objects
            .insert(object_id, Object::Dictionary(dictionary));
    }

    Ok(())
}

fn insert_pages_root(
    document: &mut Document,
    pages_id: ObjectId,
    pages_object: Object,
    ordered_page_ids: &[ObjectId],
) -> Result<(), PdfBackendError> {
    let mut dictionary = pages_object
        .as_dict()
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?
        .clone();
    dictionary.set("Count", ordered_page_ids.len() as u32);
    dictionary.remove(b"Rotate");
    dictionary.set(
        "Kids",
        ordered_page_ids
            .iter()
            .copied()
            .map(Object::Reference)
            .collect::<Vec<_>>(),
    );
    document
        .objects
        .insert(pages_id, Object::Dictionary(dictionary));

    Ok(())
}

fn insert_catalog_root(
    document: &mut Document,
    catalog_id: ObjectId,
    catalog_object: Object,
    pages_id: ObjectId,
) -> Result<(), PdfBackendError> {
    let mut dictionary = catalog_object
        .as_dict()
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?
        .clone();
    dictionary.set("Pages", pages_id);
    dictionary.remove(b"Outlines");
    document
        .objects
        .insert(catalog_id, Object::Dictionary(dictionary));

    Ok(())
}

fn apply_output_options(
    document: &mut Document,
    page_ids: &[ObjectId],
    options: PdfOutputOptions,
) -> Result<(), PdfBackendError> {
    if options.save.remove_metadata {
        remove_metadata(document);
    }

    if page_ids.is_empty() {
        return Ok(());
    }

    refresh_max_id(document);

    if options.normalize_page_size {
        normalize_page_sizes(document, page_ids)?;
    }
    Ok(())
}

pub(super) fn remove_metadata(document: &mut Document) {
    let mut object_ids = BTreeSet::new();

    remember_reference(&mut object_ids, document.trailer.remove(b"Info"));

    for (object_id, object) in document.objects.iter_mut() {
        if object.type_name().ok() == Some(b"Metadata") {
            object_ids.insert(*object_id);
        }

        let Ok(dictionary) = object.as_dict_mut() else {
            continue;
        };

        if dictionary.has_type(b"Catalog") {
            remember_reference(&mut object_ids, dictionary.remove(b"Metadata"));
        }
    }

    for object_id in object_ids {
        document.delete_object(object_id);
    }
    document.prune_objects();
}

fn remember_reference(object_ids: &mut BTreeSet<ObjectId>, object: Option<Object>) {
    if let Some(object_id) = object.and_then(|object| object.as_reference().ok()) {
        object_ids.insert(object_id);
    }
}

fn refresh_max_id(document: &mut Document) {
    document.max_id = document
        .objects
        .keys()
        .map(|(id, _)| *id)
        .max()
        .unwrap_or(1);
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

pub(super) fn save_document(
    mut document: Document,
    catalog_id: ObjectId,
    output_file: &Path,
    options: PdfSaveOptions,
) -> Result<PathBuf, PdfBackendError> {
    document.trailer.set("Root", catalog_id);
    document.max_id = document
        .objects
        .keys()
        .map(|(id, _)| *id)
        .max()
        .unwrap_or(1);
    if !options.remove_metadata {
        set_app_producer_metadata(&mut document);
    }
    document.renumber_objects();
    document.adjust_zero_pages();

    let temp_file = temporary_output_file(output_file)?;

    let write_result = if options.modern_pdf {
        std::fs::File::create(temp_file.path())
            .map_err(PdfBackendError::Save)
            .and_then(|mut file| {
                document
                    .save_modern(&mut file)
                    .map_err(|error| PdfBackendError::Write(error.to_string()))
            })
    } else {
        document
            .save(temp_file.path())
            .map(|_| ())
            .map_err(|error| PdfBackendError::Write(error.to_string()))
    };
    write_result?;

    std::fs::copy(temp_file.path(), output_file).map_err(PdfBackendError::Save)?;
    Ok(output_file.to_path_buf())
}

pub(super) fn temporary_output_file(
    output_file: &Path,
) -> Result<tempfile::NamedTempFile, PdfBackendError> {
    let name = output_file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("folios");

    tempfile::Builder::new()
        .prefix(&format!(".{name}.folios-"))
        .suffix(".pdf")
        .tempfile()
        .map_err(PdfBackendError::Save)
}
