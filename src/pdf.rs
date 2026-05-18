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
    NoPagesSelected,
    OutputMatchesInput,
    Load { path: PathBuf, message: String },
    InvalidPageRange(String),
    InvalidDocument(String),
    Write(String),
    Save(std::io::Error),
    WorkerStopped,
}

impl fmt::Display for PdfBackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEnoughInputs => write!(f, "Choose at least two PDF files to merge."),
            Self::NoPagesSelected => write!(f, "Choose at least one page to save."),
            Self::OutputMatchesInput => {
                write!(f, "Save the PDF as a new file, not over the input file.")
            }
            Self::Load { path, message } => write!(
                f,
                "Could not read {}: {message}",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("PDF")
            ),
            Self::InvalidPageRange(message) => write!(f, "{message}"),
            Self::InvalidDocument(message) => write!(f, "Could not process this PDF: {message}"),
            Self::Write(message) => write!(f, "Could not write the PDF: {message}"),
            Self::Save(error) => write!(f, "Could not save the PDF: {error}"),
            Self::WorkerStopped => write!(f, "The PDF operation stopped unexpectedly."),
        }
    }
}

pub async fn page_count(input_file: PathBuf) -> Result<usize, PdfBackendError> {
    gio::spawn_blocking(move || page_count_blocking(&input_file))
        .await
        .unwrap_or(Err(PdfBackendError::WorkerStopped))
}

pub async fn merge_pdfs(
    input_files: Vec<PathBuf>,
    output_file: PathBuf,
) -> Result<PathBuf, PdfBackendError> {
    gio::spawn_blocking(move || merge_pdfs_blocking(input_files, output_file))
        .await
        .unwrap_or(Err(PdfBackendError::WorkerStopped))
}

pub async fn organize_pdf(
    input_file: PathBuf,
    page_order: Vec<u32>,
    output_file: PathBuf,
) -> Result<PathBuf, PdfBackendError> {
    gio::spawn_blocking(move || write_selected_pages(input_file, page_order, output_file))
        .await
        .unwrap_or(Err(PdfBackendError::WorkerStopped))
}

pub async fn extract_pages(
    input_file: PathBuf,
    pages: Vec<u32>,
    output_file: PathBuf,
) -> Result<PathBuf, PdfBackendError> {
    gio::spawn_blocking(move || write_selected_pages(input_file, pages, output_file))
        .await
        .unwrap_or(Err(PdfBackendError::WorkerStopped))
}

pub fn parse_page_ranges(input: &str, page_count: usize) -> Result<Vec<u32>, PdfBackendError> {
    let mut pages = Vec::new();
    let input = input.trim();

    if input.is_empty() {
        return Err(PdfBackendError::NoPagesSelected);
    }

    for part in input.split(',').map(str::trim) {
        if part.is_empty() {
            return Err(PdfBackendError::InvalidPageRange(
                "Enter page ranges like 1,3-5,8.".to_string(),
            ));
        }

        if let Some((start, end)) = part.split_once('-') {
            let start = parse_page_number(start, page_count)?;
            let end = parse_page_number(end, page_count)?;

            if start > end {
                return Err(PdfBackendError::InvalidPageRange(format!(
                    "Page range {start}-{end} is backwards."
                )));
            }

            pages.extend(start..=end);
        } else {
            pages.push(parse_page_number(part, page_count)?);
        }
    }

    Ok(pages)
}

fn page_count_blocking(input_file: &Path) -> Result<usize, PdfBackendError> {
    let document = Document::load(input_file).map_err(|error| PdfBackendError::Load {
        path: input_file.to_path_buf(),
        message: error.to_string(),
    })?;

    Ok(document.get_pages().len())
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

fn write_selected_pages(
    input_file: PathBuf,
    page_numbers: Vec<u32>,
    output_file: PathBuf,
) -> Result<PathBuf, PdfBackendError> {
    if page_numbers.is_empty() {
        return Err(PdfBackendError::NoPagesSelected);
    }

    if input_file == output_file {
        return Err(PdfBackendError::OutputMatchesInput);
    }

    let document = Document::load(&input_file).map_err(|error| PdfBackendError::Load {
        path: input_file.clone(),
        message: error.to_string(),
    })?;
    let pages = document.get_pages();
    let mut selected_ids = Vec::with_capacity(page_numbers.len());
    let mut page_objects = BTreeMap::new();

    for page_number in page_numbers {
        let object_id = pages.get(&page_number).ok_or_else(|| {
            PdfBackendError::InvalidPageRange(format!("Page {page_number} is not in this PDF."))
        })?;
        let object = document
            .get_object(*object_id)
            .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?
            .to_owned();
        page_objects.insert(*object_id, object);
        selected_ids.push(*object_id);
    }

    let temp_file = temporary_output_path(&output_file);
    let mut output = Document::with_version("1.5");
    let (catalog_id, catalog_object, pages_id, pages_object) =
        collect_document_roots(document.objects, &mut output)?;

    for (object_id, object) in page_objects {
        let mut dictionary = object
            .as_dict()
            .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?
            .clone();
        dictionary.set("Parent", pages_id);
        output
            .objects
            .insert(object_id, Object::Dictionary(dictionary));
    }

    let mut pages_dictionary = pages_object
        .as_dict()
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?
        .clone();
    pages_dictionary.set("Count", selected_ids.len() as u32);
    pages_dictionary.set(
        "Kids",
        selected_ids
            .into_iter()
            .map(Object::Reference)
            .collect::<Vec<_>>(),
    );
    output
        .objects
        .insert(pages_id, Object::Dictionary(pages_dictionary));

    let mut catalog_dictionary = catalog_object
        .as_dict()
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?
        .clone();
    catalog_dictionary.set("Pages", pages_id);
    catalog_dictionary.remove(b"Outlines");
    output
        .objects
        .insert(catalog_id, Object::Dictionary(catalog_dictionary));

    save_document(output, catalog_id, &temp_file, &output_file)
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

fn save_document(
    mut document: Document,
    catalog_id: ObjectId,
    temp_file: &Path,
    output_file: &Path,
) -> Result<PathBuf, PdfBackendError> {
    document.trailer.set("Root", catalog_id);
    document.max_id = document
        .objects
        .keys()
        .map(|(id, _)| *id)
        .max()
        .unwrap_or(1);
    document.renumber_objects();
    document.adjust_zero_pages();

    document
        .save(temp_file)
        .map_err(|error| PdfBackendError::Write(error.to_string()))?;

    std::fs::rename(temp_file, output_file).map_err(PdfBackendError::Save)?;
    Ok(output_file.to_path_buf())
}

fn parse_page_number(input: &str, page_count: usize) -> Result<u32, PdfBackendError> {
    let page = input
        .trim()
        .parse::<u32>()
        .map_err(|_| PdfBackendError::InvalidPageRange(format!("{input} is not a page number.")))?;

    if page == 0 || page as usize > page_count {
        return Err(PdfBackendError::InvalidPageRange(format!(
            "Page {page} is not in this PDF."
        )));
    }

    Ok(page)
}

fn temporary_output_path(output_file: &Path) -> PathBuf {
    let directory = output_file.parent().unwrap_or_else(|| Path::new("."));
    let name = output_file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("folios");

    directory.join(format!(".{name}.folios-{}.pdf", std::process::id()))
}
