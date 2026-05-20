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

const PAGE_RANGE_HINT: &str = "Enter page ranges like 1,3-5,8.";
const PAGE_LIST_HINT: &str = "Enter pages like 2,4,7.";

#[derive(Clone, Debug)]
pub struct PdfInput {
    pub path: PathBuf,
    pub rotation: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct PageSelection {
    pub page_number: u32,
    pub rotation: i64,
}

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

#[derive(Clone, Copy, Debug)]
pub struct CompressOptions {
    pub remove_empty_streams: bool,
    pub prune_objects: bool,
}

#[derive(Clone, Debug)]
pub enum SplitRule {
    EveryPage,
    EvenPages,
    OddPages,
    SpecificPages(Vec<u32>),
    EveryNPages(u32),
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

pub async fn merge_pdfs(
    input_files: Vec<PdfInput>,
    output_file: PathBuf,
) -> Result<PathBuf, PdfBackendError> {
    gio::spawn_blocking(move || merge_pdfs_blocking(input_files, output_file))
        .await
        .unwrap_or(Err(PdfBackendError::WorkerStopped))
}

pub async fn organize_pdf(
    input_file: PathBuf,
    page_order: Vec<PageSelection>,
    output_file: PathBuf,
) -> Result<PathBuf, PdfBackendError> {
    gio::spawn_blocking(move || write_selected_pages(input_file, page_order, output_file))
        .await
        .unwrap_or(Err(PdfBackendError::WorkerStopped))
}

pub async fn extract_pages(
    input_file: PathBuf,
    pages: Vec<PageSelection>,
    output_file: PathBuf,
) -> Result<PathBuf, PdfBackendError> {
    gio::spawn_blocking(move || write_selected_pages(input_file, pages, output_file))
        .await
        .unwrap_or(Err(PdfBackendError::WorkerStopped))
}

pub async fn split_pdf(
    input_file: PathBuf,
    output_folder: PathBuf,
    prefix: String,
    rule: SplitRule,
) -> Result<PathBuf, PdfBackendError> {
    gio::spawn_blocking(move || split_pdf_blocking(input_file, output_folder, prefix, rule))
        .await
        .unwrap_or(Err(PdfBackendError::WorkerStopped))
}

pub async fn compress_pdf(
    input_file: PathBuf,
    output_file: PathBuf,
    options: CompressOptions,
) -> Result<PathBuf, PdfBackendError> {
    gio::spawn_blocking(move || compress_pdf_blocking(input_file, output_file, options))
        .await
        .unwrap_or(Err(PdfBackendError::WorkerStopped))
}

pub fn parse_page_ranges(input: &str, page_count: usize) -> Result<Vec<u32>, PdfBackendError> {
    parse_page_list(input, page_count, true, PdfBackendError::NoPagesSelected)
}

pub fn parse_page_numbers(input: &str, page_count: usize) -> Result<Vec<u32>, PdfBackendError> {
    parse_page_list(
        input,
        page_count,
        false,
        PdfBackendError::InvalidPageRange(PAGE_LIST_HINT.to_string()),
    )
}

fn merge_pdfs_blocking(
    input_files: Vec<PdfInput>,
    output_file: PathBuf,
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
        let mut document = Document::load(&input.path).map_err(|error| PdfBackendError::Load {
            path: input.path.clone(),
            message: error.to_string(),
        })?;

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

    build_and_save_document(document_objects, output_pages, &output_file)
}

fn write_selected_pages(
    input_file: PathBuf,
    page_numbers: Vec<PageSelection>,
    output_file: PathBuf,
) -> Result<PathBuf, PdfBackendError> {
    let document = Document::load(&input_file).map_err(|error| PdfBackendError::Load {
        path: input_file.clone(),
        message: error.to_string(),
    })?;

    write_selected_pages_from_document(&input_file, &document, page_numbers, &output_file)
}

fn write_selected_pages_from_document(
    input_file: &Path,
    document: &Document,
    page_numbers: Vec<PageSelection>,
    output_file: &Path,
) -> Result<PathBuf, PdfBackendError> {
    if page_numbers.is_empty() {
        return Err(PdfBackendError::NoPagesSelected);
    }

    if input_file == output_file {
        return Err(PdfBackendError::OutputMatchesInput);
    }

    let pages = document.get_pages();
    let mut output_pages = OutputPages::with_capacity(page_numbers.len());

    for selection in page_numbers {
        let object_id = pages.get(&selection.page_number).ok_or_else(|| {
            PdfBackendError::InvalidPageRange(format!(
                "Page {} is not in this PDF.",
                selection.page_number
            ))
        })?;
        output_pages.push(
            *object_id,
            rotated_page_object(document, *object_id, selection.rotation)?,
        );
    }

    build_and_save_document(document.objects.clone(), output_pages, output_file)
}

fn split_pdf_blocking(
    input_file: PathBuf,
    output_folder: PathBuf,
    prefix: String,
    rule: SplitRule,
) -> Result<PathBuf, PdfBackendError> {
    let document = Document::load(&input_file).map_err(|error| PdfBackendError::Load {
        path: input_file.clone(),
        message: error.to_string(),
    })?;
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
        write_selected_pages_from_document(&input_file, &document, page_numbers, &output_file)?;
        start = end + 1;
        index += 1;
    }

    if start <= page_count {
        let output_file = output_folder.join(format!("{} {}.pdf", prefix, index));
        let page_numbers = page_selections(start..=page_count);
        write_selected_pages_from_document(&input_file, &document, page_numbers, &output_file)?;
    }

    Ok(output_folder)
}

fn page_selections(pages: std::ops::RangeInclusive<u32>) -> Vec<PageSelection> {
    pages
        .map(|page_number| PageSelection {
            page_number,
            rotation: 0,
        })
        .collect()
}

fn inherited_page_rotation(document: &Document, mut object_id: ObjectId) -> i64 {
    for _ in 0..document.objects.len() {
        let Ok(dictionary) = document.get_object(object_id).and_then(Object::as_dict) else {
            break;
        };

        if let Ok(rotation) = dictionary.get(b"Rotate").and_then(Object::as_i64) {
            return normalize_rotation(rotation);
        }

        let Ok(parent_id) = dictionary.get(b"Parent").and_then(Object::as_reference) else {
            break;
        };
        object_id = parent_id;
    }

    0
}

fn set_page_rotation(dictionary: &mut lopdf::Dictionary, current_rotation: i64, rotation: i64) {
    let rotation = normalize_rotation(current_rotation + rotation);
    if rotation == 0 {
        dictionary.remove(b"Rotate");
    } else {
        dictionary.set("Rotate", rotation);
    }
}

fn normalize_rotation(rotation: i64) -> i64 {
    rotation.rem_euclid(360)
}

fn split_breaks(rule: SplitRule, page_count: u32) -> Result<Vec<u32>, PdfBackendError> {
    let validate_page_numbers = matches!(rule, SplitRule::SpecificPages(_));
    let mut breaks: Vec<u32> = match rule {
        SplitRule::EveryPage => (1..=page_count).collect(),
        SplitRule::EvenPages => (2..=page_count).step_by(2).collect(),
        SplitRule::OddPages => (1..=page_count).step_by(2).collect(),
        SplitRule::SpecificPages(pages) => {
            if pages.is_empty() {
                return Err(PdfBackendError::InvalidPageRange(
                    PAGE_LIST_HINT.to_string(),
                ));
            }
            pages
        }
        SplitRule::EveryNPages(pages) => {
            if pages == 0 {
                return Err(PdfBackendError::InvalidPageRange(
                    "Enter a page count of 1 or more.".to_string(),
                ));
            }
            (pages..=page_count).step_by(pages as usize).collect()
        }
    };

    if validate_page_numbers {
        for page in &breaks {
            if *page == 0 || *page > page_count {
                return Err(PdfBackendError::InvalidPageRange(format!(
                    "Page {page} is not in this PDF."
                )));
            }
        }
    }

    breaks.sort_unstable();
    breaks.dedup();
    Ok(breaks)
}

fn parse_page_list(
    input: &str,
    page_count: usize,
    allow_ranges: bool,
    empty_error: PdfBackendError,
) -> Result<Vec<u32>, PdfBackendError> {
    let mut pages = Vec::new();
    let input = input.trim();

    if input.is_empty() {
        return Err(empty_error);
    }

    for part in input.split(',').map(str::trim) {
        if part.is_empty() {
            return Err(PdfBackendError::InvalidPageRange(
                page_input_hint(allow_ranges).to_string(),
            ));
        }

        if allow_ranges {
            pages.extend(parse_page_range_part(part, page_count)?);
        } else {
            pages.push(parse_page_number(part, page_count)?);
        }
    }

    Ok(pages)
}

fn parse_page_range_part(input: &str, page_count: usize) -> Result<Vec<u32>, PdfBackendError> {
    if let Some((start, end)) = input.split_once('-') {
        let start = parse_page_number(start, page_count)?;
        let end = parse_page_number(end, page_count)?;

        if start > end {
            return Err(PdfBackendError::InvalidPageRange(format!(
                "Page range {start}-{end} is backwards."
            )));
        }

        Ok((start..=end).collect())
    } else {
        parse_page_number(input, page_count).map(|page| vec![page])
    }
}

fn page_input_hint(allow_ranges: bool) -> &'static str {
    if allow_ranges {
        PAGE_RANGE_HINT
    } else {
        PAGE_LIST_HINT
    }
}

fn compress_pdf_blocking(
    input_file: PathBuf,
    output_file: PathBuf,
    options: CompressOptions,
) -> Result<PathBuf, PdfBackendError> {
    if input_file == output_file {
        return Err(PdfBackendError::OutputMatchesInput);
    }

    let mut document = Document::load(&input_file).map_err(|error| PdfBackendError::Load {
        path: input_file.clone(),
        message: error.to_string(),
    })?;
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
    document.compress();

    save_document(document, catalog_id, &output_file)
}

struct OutputPages {
    objects: BTreeMap<ObjectId, Object>,
    ordered_ids: Vec<ObjectId>,
}

impl OutputPages {
    fn new() -> Self {
        Self {
            objects: BTreeMap::new(),
            ordered_ids: Vec::new(),
        }
    }

    fn with_capacity(capacity: usize) -> Self {
        Self {
            objects: BTreeMap::new(),
            ordered_ids: Vec::with_capacity(capacity),
        }
    }

    fn push(&mut self, object_id: ObjectId, object: Object) {
        self.objects.insert(object_id, object);
        self.ordered_ids.push(object_id);
    }
}

fn build_and_save_document(
    document_objects: BTreeMap<ObjectId, Object>,
    output_pages: OutputPages,
    output_file: &Path,
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

    save_document(output, catalog_id, output_file)
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

fn rotated_page_object(
    document: &Document,
    object_id: ObjectId,
    rotation: i64,
) -> Result<Object, PdfBackendError> {
    let page_rotation = inherited_page_rotation(document, object_id);
    let object = document
        .get_object(object_id)
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?
        .to_owned();
    let mut dictionary = object
        .as_dict()
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?
        .clone();
    set_page_rotation(&mut dictionary, page_rotation, rotation);

    Ok(Object::Dictionary(dictionary))
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

    let temp_file = temporary_output_file(output_file)?;

    if let Err(error) = document.save(temp_file.path()) {
        return Err(PdfBackendError::Write(error.to_string()));
    }

    std::fs::copy(temp_file.path(), output_file).map_err(PdfBackendError::Save)?;
    Ok(output_file.to_path_buf())
}

fn temporary_output_file(output_file: &Path) -> Result<tempfile::NamedTempFile, PdfBackendError> {
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

fn split_output_prefix(input_file: &Path, prefix: &str) -> String {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        input_file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Split")
            .to_string()
    } else {
        prefix.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos();
            let directory_name = format!("folios-{name}-{}-{unique}", std::process::id());
            let path = std::env::temp_dir().join(directory_name);

            fs::create_dir(&path).expect("test directory should be created");
            Self { path }
        }

        fn join(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn parse_page_ranges_accepts_single_pages_and_ranges() {
        assert_eq!(parse_page_ranges("1", 10).unwrap(), vec![1]);
        assert_eq!(parse_page_ranges("1,3-5", 10).unwrap(), vec![1, 3, 4, 5]);
        assert_eq!(
            parse_page_ranges("2, 4-8, 10", 10).unwrap(),
            vec![2, 4, 5, 6, 7, 8, 10]
        );
    }

    #[test]
    fn parse_page_ranges_rejects_invalid_input() {
        assert_error(
            parse_page_ranges("", 10),
            "Choose at least one page to save.",
        );
        assert_error(parse_page_ranges("0", 10), "Page 0 is not in this PDF.");
        assert_error(parse_page_ranges("abc", 10), "abc is not a page number.");
        assert_error(parse_page_ranges("5-3", 10), "Page range 5-3 is backwards.");
        assert_error(parse_page_ranges("11", 10), "Page 11 is not in this PDF.");
        assert_error(
            parse_page_ranges("1,,3", 10),
            "Enter page ranges like 1,3-5,8.",
        );
    }

    #[test]
    fn parse_page_numbers_accepts_comma_only_pages() {
        assert_eq!(parse_page_numbers("2,4,8", 10).unwrap(), vec![2, 4, 8]);

        assert_error(parse_page_numbers("", 10), "Enter pages like 2,4,7.");
        assert_error(parse_page_numbers("2-4", 10), "2-4 is not a page number.");
        assert_error(parse_page_numbers("2,,4", 10), "Enter pages like 2,4,7.");
    }

    #[test]
    fn merge_pdfs_preserves_page_count_and_order() {
        let dir = TestDir::new("merge");
        let first = dir.join("first.pdf");
        let second = dir.join("second.pdf");
        let output = dir.join("merged.pdf");
        write_test_pdf(&first, &[10, 20]);
        write_test_pdf(&second, &[30]);

        let result = merge_pdfs_blocking(
            vec![pdf_input(first.clone(), 0), pdf_input(second.clone(), 0)],
            output.clone(),
        );

        assert_eq!(result.unwrap(), output);
        assert_eq!(page_markers(&output), vec![10, 20, 30]);
    }

    #[test]
    fn merge_pdfs_rejects_too_few_inputs_and_input_overwrite() {
        let dir = TestDir::new("merge-invalid");
        let input = dir.join("input.pdf");
        write_test_pdf(&input, &[10]);

        assert_error(
            merge_pdfs_blocking(vec![pdf_input(input.clone(), 0)], dir.join("out.pdf")),
            "Choose at least two PDF files to merge.",
        );
        assert_error(
            merge_pdfs_blocking(
                vec![
                    pdf_input(input.clone(), 0),
                    pdf_input(dir.join("other.pdf"), 0),
                ],
                input,
            ),
            "Save the PDF as a new file, not over the input file.",
        );
    }

    #[test]
    fn merge_pdfs_rotates_all_pages_from_input_file() {
        let dir = TestDir::new("merge-rotation");
        let first = dir.join("first.pdf");
        let second = dir.join("second.pdf");
        let output = dir.join("merged.pdf");
        write_test_pdf(&first, &[10, 20]);
        write_test_pdf(&second, &[30]);

        let result = merge_pdfs_blocking(
            vec![pdf_input(first, 90), pdf_input(second, 180)],
            output.clone(),
        );

        assert_eq!(result.unwrap(), output);
        assert_eq!(page_rotations(&output), vec![90, 90, 180]);
    }

    #[test]
    fn write_selected_pages_uses_requested_order() {
        let dir = TestDir::new("selected-pages");
        let input = dir.join("input.pdf");
        let output = dir.join("output.pdf");
        write_test_pdf(&input, &[10, 20, 30]);

        let result = write_selected_pages(
            input.clone(),
            vec![page_selection(3, 0), page_selection(1, 0)],
            output.clone(),
        );

        assert_eq!(result.unwrap(), output);
        assert_eq!(page_markers(&output), vec![30, 10]);
    }

    #[test]
    fn write_selected_pages_rotates_requested_pages() {
        let dir = TestDir::new("selected-pages-rotation");
        let input = dir.join("input.pdf");
        let output = dir.join("output.pdf");
        write_test_pdf(&input, &[10, 20, 30]);

        let result = write_selected_pages(
            input.clone(),
            vec![page_selection(3, 90), page_selection(1, 270)],
            output.clone(),
        );

        assert_eq!(result.unwrap(), output);
        assert_eq!(page_markers(&output), vec![30, 10]);
        assert_eq!(page_rotations(&output), vec![90, 270]);
    }

    #[test]
    fn write_selected_pages_rejects_empty_pages_and_input_overwrite() {
        let dir = TestDir::new("selected-pages-invalid");
        let input = dir.join("input.pdf");
        write_test_pdf(&input, &[10, 20]);

        assert_error(
            write_selected_pages(input.clone(), Vec::new(), dir.join("empty.pdf")),
            "Choose at least one page to save.",
        );
        assert_error(
            write_selected_pages(input.clone(), vec![page_selection(1, 0)], input.clone()),
            "Save the PDF as a new file, not over the input file.",
        );
        assert_error(
            write_selected_pages(input, vec![page_selection(3, 0)], dir.join("missing.pdf")),
            "Page 3 is not in this PDF.",
        );
    }

    #[test]
    fn split_pdf_creates_expected_files_for_every_n_pages() {
        let dir = TestDir::new("split");
        let input = dir.join("input.pdf");
        let output_folder = dir.join("parts");
        fs::create_dir(&output_folder).expect("output folder should be created");
        write_test_pdf(&input, &[10, 20, 30, 40, 50]);

        let result = split_pdf_blocking(
            input,
            output_folder.clone(),
            "Chapter".to_string(),
            SplitRule::EveryNPages(2),
        );

        assert_eq!(result.unwrap(), output_folder);
        assert_split_outputs(
            &dir.join("parts"),
            &[
                ("Chapter 1.pdf", &[10, 20]),
                ("Chapter 2.pdf", &[30, 40]),
                ("Chapter 3.pdf", &[50]),
            ],
        );
    }

    #[test]
    fn split_pdf_creates_one_file_per_page() {
        let dir = TestDir::new("split-every-page");
        let input = dir.join("input.pdf");
        let output_folder = dir.join("parts");
        fs::create_dir(&output_folder).expect("output folder should be created");
        write_test_pdf(&input, &[10, 20, 30]);

        let result = split_pdf_blocking(
            input,
            output_folder.clone(),
            "Page".to_string(),
            SplitRule::EveryPage,
        );

        assert_eq!(result.unwrap(), output_folder);
        assert_split_outputs(
            &dir.join("parts"),
            &[
                ("Page 1.pdf", &[10]),
                ("Page 2.pdf", &[20]),
                ("Page 3.pdf", &[30]),
            ],
        );
    }

    #[test]
    fn split_pdf_groups_even_page_breaks_with_trailing_remainder() {
        let dir = TestDir::new("split-even");
        let input = dir.join("input.pdf");
        let output_folder = dir.join("parts");
        fs::create_dir(&output_folder).expect("output folder should be created");
        write_test_pdf(&input, &[10, 20, 30, 40, 50]);

        let result = split_pdf_blocking(
            input,
            output_folder.clone(),
            "Even".to_string(),
            SplitRule::EvenPages,
        );

        assert_eq!(result.unwrap(), output_folder);
        assert_split_outputs(
            &dir.join("parts"),
            &[
                ("Even 1.pdf", &[10, 20]),
                ("Even 2.pdf", &[30, 40]),
                ("Even 3.pdf", &[50]),
            ],
        );
    }

    #[test]
    fn split_pdf_groups_odd_page_breaks() {
        let dir = TestDir::new("split-odd");
        let input = dir.join("input.pdf");
        let output_folder = dir.join("parts");
        fs::create_dir(&output_folder).expect("output folder should be created");
        write_test_pdf(&input, &[10, 20, 30, 40, 50]);

        let result = split_pdf_blocking(
            input,
            output_folder.clone(),
            "Odd".to_string(),
            SplitRule::OddPages,
        );

        assert_eq!(result.unwrap(), output_folder);
        assert_split_outputs(
            &dir.join("parts"),
            &[
                ("Odd 1.pdf", &[10]),
                ("Odd 2.pdf", &[20, 30]),
                ("Odd 3.pdf", &[40, 50]),
            ],
        );
    }

    #[test]
    fn split_pdf_sorts_and_deduplicates_specific_page_breaks() {
        let dir = TestDir::new("split-specific");
        let input = dir.join("input.pdf");
        let output_folder = dir.join("parts");
        fs::create_dir(&output_folder).expect("output folder should be created");
        write_test_pdf(&input, &[10, 20, 30, 40, 50]);

        let result = split_pdf_blocking(
            input,
            output_folder.clone(),
            "Specific".to_string(),
            SplitRule::SpecificPages(vec![4, 2, 2]),
        );

        assert_eq!(result.unwrap(), output_folder);
        assert_split_outputs(
            &dir.join("parts"),
            &[
                ("Specific 1.pdf", &[10, 20]),
                ("Specific 2.pdf", &[30, 40]),
                ("Specific 3.pdf", &[50]),
            ],
        );
    }

    #[test]
    fn split_pdf_uses_input_stem_for_blank_prefix() {
        let dir = TestDir::new("split-default-prefix");
        let input = dir.join("source document.pdf");
        let output_folder = dir.join("parts");
        fs::create_dir(&output_folder).expect("output folder should be created");
        write_test_pdf(&input, &[10, 20]);

        let result = split_pdf_blocking(
            input,
            output_folder.clone(),
            "   ".to_string(),
            SplitRule::EveryPage,
        );

        assert_eq!(result.unwrap(), output_folder);
        assert_split_outputs(
            &dir.join("parts"),
            &[
                ("source document 1.pdf", &[10]),
                ("source document 2.pdf", &[20]),
            ],
        );
    }

    #[test]
    fn split_pdf_rejects_empty_pdf() {
        let dir = TestDir::new("split-empty");
        let input = dir.join("empty.pdf");
        let output_folder = dir.join("parts");
        fs::create_dir(&output_folder).expect("output folder should be created");
        write_test_pdf(&input, &[]);

        assert_error(
            split_pdf_blocking(
                input,
                output_folder,
                "Empty".to_string(),
                SplitRule::EveryPage,
            ),
            "Choose at least one page to save.",
        );
    }

    #[test]
    fn split_pdf_reports_load_error_with_input_filename() {
        let dir = TestDir::new("split-corrupt");
        let input = dir.join("broken.pdf");
        let output_folder = dir.join("parts");
        fs::create_dir(&output_folder).expect("output folder should be created");
        fs::write(&input, b"not a pdf").expect("corrupt PDF fixture should be written");

        let error = split_pdf_blocking(
            input,
            output_folder,
            "Broken".to_string(),
            SplitRule::EveryPage,
        )
        .unwrap_err()
        .to_string();

        assert!(error.starts_with("Could not read broken.pdf:"));
    }

    #[test]
    fn split_pdf_failed_write_does_not_create_requested_output() {
        let dir = TestDir::new("split-write-failure");
        let input = dir.join("input.pdf");
        let missing_output_folder = dir.join("missing").join("parts");
        let requested_output = missing_output_folder.join("Split 1.pdf");
        write_test_pdf(&input, &[10]);

        let result = split_pdf_blocking(
            input,
            missing_output_folder,
            "Split".to_string(),
            SplitRule::EveryPage,
        );

        assert!(matches!(result, Err(PdfBackendError::Save(_))));
        assert!(!requested_output.exists());
    }

    #[test]
    fn split_breaks_normalizes_and_rejects_invalid_rules() {
        assert_eq!(
            split_breaks(SplitRule::EveryPage, 3).unwrap(),
            vec![1, 2, 3]
        );
        assert_eq!(split_breaks(SplitRule::EvenPages, 5).unwrap(), vec![2, 4]);
        assert_eq!(split_breaks(SplitRule::OddPages, 5).unwrap(), vec![1, 3, 5]);
        assert_eq!(
            split_breaks(SplitRule::SpecificPages(vec![4, 2, 2]), 5).unwrap(),
            vec![2, 4]
        );

        assert_error(
            split_breaks(SplitRule::SpecificPages(Vec::new()), 5),
            "Enter pages like 2,4,7.",
        );
        assert_error(
            split_breaks(SplitRule::EveryNPages(0), 5),
            "Enter a page count of 1 or more.",
        );
        assert_error(
            split_breaks(SplitRule::SpecificPages(vec![6]), 5),
            "Page 6 is not in this PDF.",
        );
    }

    #[test]
    fn compress_pdf_writes_valid_output_and_rejects_input_overwrite() {
        let dir = TestDir::new("compress");
        let input = dir.join("input.pdf");
        let output = dir.join("compressed.pdf");
        write_test_pdf(&input, &[10, 20]);

        let result = compress_pdf_blocking(
            input.clone(),
            output.clone(),
            CompressOptions {
                remove_empty_streams: true,
                prune_objects: true,
            },
        );

        assert_eq!(result.unwrap(), output);
        assert_eq!(page_markers(&output), vec![10, 20]);
        assert_error(
            compress_pdf_blocking(
                input.clone(),
                input,
                CompressOptions {
                    remove_empty_streams: false,
                    prune_objects: false,
                },
            ),
            "Save the PDF as a new file, not over the input file.",
        );
    }

    fn assert_error<T: std::fmt::Debug>(result: Result<T, PdfBackendError>, message: &str) {
        assert_eq!(result.unwrap_err().to_string(), message);
    }

    fn assert_split_outputs(folder: &Path, expected: &[(&str, &[i64])]) {
        let files = sorted_pdf_files(folder);
        let actual_names = files
            .iter()
            .map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .expect("test output should have a UTF-8 filename")
                    .to_string()
            })
            .collect::<Vec<_>>();
        let expected_names = expected
            .iter()
            .map(|(name, _)| name.to_string())
            .collect::<Vec<_>>();

        assert_eq!(actual_names, expected_names);
        for (file, (_, expected_markers)) in files.iter().zip(expected.iter()) {
            assert_eq!(page_markers(file), *expected_markers);
        }
    }

    fn write_test_pdf(path: &Path, page_markers: &[i64]) {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let mut kids = Vec::with_capacity(page_markers.len());

        for marker in page_markers {
            let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
            let page_id = document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "Resources" => dictionary! {},
                "MediaBox" => vec![0.into(), 0.into(), (*marker).into(), 100.into()],
            });
            kids.push(page_id.into());
        }

        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => page_markers.len() as i64,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document.compress();
        document.save(path).expect("test PDF should be saved");
    }

    fn pdf_input(path: PathBuf, rotation: i64) -> PdfInput {
        PdfInput { path, rotation }
    }

    fn page_selection(page_number: u32, rotation: i64) -> PageSelection {
        PageSelection {
            page_number,
            rotation,
        }
    }

    fn page_markers(path: &Path) -> Vec<i64> {
        let document = Document::load(path).expect("test PDF should load");
        document
            .get_pages()
            .into_values()
            .map(|object_id| {
                let page = document
                    .get_object(object_id)
                    .expect("page object should exist")
                    .as_dict()
                    .expect("page should be a dictionary");
                let media_box = page
                    .get(b"MediaBox")
                    .expect("page should have a media box")
                    .as_array()
                    .expect("media box should be an array");

                media_box[2]
                    .as_i64()
                    .expect("media box marker should be an integer")
            })
            .collect()
    }

    fn page_rotations(path: &Path) -> Vec<i64> {
        let document = Document::load(path).expect("test PDF should load");
        document
            .get_pages()
            .into_values()
            .map(|object_id| {
                document
                    .get_object(object_id)
                    .expect("page object should exist")
                    .as_dict()
                    .expect("page should be a dictionary")
                    .get(b"Rotate")
                    .and_then(Object::as_i64)
                    .unwrap_or(0)
            })
            .collect()
    }

    fn sorted_pdf_files(folder: &Path) -> Vec<PathBuf> {
        let mut files = fs::read_dir(folder)
            .expect("folder should exist")
            .map(|entry| entry.expect("entry should be readable").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "pdf"))
            .collect::<Vec<_>>();
        files.sort();
        files
    }
}
