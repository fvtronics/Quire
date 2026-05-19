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

pub fn parse_page_numbers(input: &str, page_count: usize) -> Result<Vec<u32>, PdfBackendError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(PdfBackendError::InvalidPageRange(
            "Enter pages like 2,4,7.".to_string(),
        ));
    }

    input
        .split(',')
        .map(str::trim)
        .map(|part| {
            if part.is_empty() {
                Err(PdfBackendError::InvalidPageRange(
                    "Enter pages like 2,4,7.".to_string(),
                ))
            } else {
                parse_page_number(part, page_count)
            }
        })
        .collect()
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
    let document = Document::load(&input_file).map_err(|error| PdfBackendError::Load {
        path: input_file.clone(),
        message: error.to_string(),
    })?;

    write_selected_pages_from_document(&input_file, &document, page_numbers, &output_file)
}

fn write_selected_pages_from_document(
    input_file: &Path,
    document: &Document,
    page_numbers: Vec<u32>,
    output_file: &Path,
) -> Result<PathBuf, PdfBackendError> {
    if page_numbers.is_empty() {
        return Err(PdfBackendError::NoPagesSelected);
    }

    if input_file == output_file {
        return Err(PdfBackendError::OutputMatchesInput);
    }

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

    let temp_file = temporary_output_path(output_file);
    let mut output = Document::with_version("1.5");
    let (catalog_id, catalog_object, pages_id, pages_object) =
        collect_document_roots(document.objects.clone(), &mut output)?;

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

    save_document(output, catalog_id, &temp_file, output_file)
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
        let page_numbers = (start..=end).collect();
        write_selected_pages_from_document(&input_file, &document, page_numbers, &output_file)?;
        start = end + 1;
        index += 1;
    }

    if start <= page_count {
        let output_file = output_folder.join(format!("{} {}.pdf", prefix, index));
        let page_numbers = (start..=page_count).collect();
        write_selected_pages_from_document(&input_file, &document, page_numbers, &output_file)?;
    }

    Ok(output_folder)
}

fn split_breaks(rule: SplitRule, page_count: u32) -> Result<Vec<u32>, PdfBackendError> {
    let mut breaks: Vec<u32> = match rule {
        SplitRule::EveryPage => (1..=page_count).collect(),
        SplitRule::EvenPages => (2..=page_count).step_by(2).collect(),
        SplitRule::OddPages => (1..=page_count).step_by(2).collect(),
        SplitRule::SpecificPages(pages) => {
            if pages.is_empty() {
                return Err(PdfBackendError::InvalidPageRange(
                    "Enter pages like 2,4,7.".to_string(),
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

    for page in &breaks {
        if *page == 0 || *page > page_count {
            return Err(PdfBackendError::InvalidPageRange(format!(
                "Page {page} is not in this PDF."
            )));
        }
    }

    breaks.sort_unstable();
    breaks.dedup();
    Ok(breaks)
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
    let temp_file = temporary_output_path(&output_file);

    if options.remove_empty_streams {
        document.delete_zero_length_streams();
    }
    if options.prune_objects {
        document.prune_objects();
    }
    document.compress();

    save_document(document, catalog_id, &temp_file, &output_file)
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

        let result = merge_pdfs_blocking(vec![first.clone(), second.clone()], output.clone());

        assert_eq!(result.unwrap(), output);
        assert_eq!(page_markers(&output), vec![10, 20, 30]);
        assert!(!temporary_output_path(&output).exists());
    }

    #[test]
    fn merge_pdfs_rejects_too_few_inputs_and_input_overwrite() {
        let dir = TestDir::new("merge-invalid");
        let input = dir.join("input.pdf");
        write_test_pdf(&input, &[10]);

        assert_error(
            merge_pdfs_blocking(vec![input.clone()], dir.join("out.pdf")),
            "Choose at least two PDF files to merge.",
        );
        assert_error(
            merge_pdfs_blocking(vec![input.clone(), dir.join("other.pdf")], input),
            "Save the PDF as a new file, not over the input file.",
        );
    }

    #[test]
    fn write_selected_pages_uses_requested_order() {
        let dir = TestDir::new("selected-pages");
        let input = dir.join("input.pdf");
        let output = dir.join("output.pdf");
        write_test_pdf(&input, &[10, 20, 30]);

        let result = write_selected_pages(input.clone(), vec![3, 1], output.clone());

        assert_eq!(result.unwrap(), output);
        assert_eq!(page_markers(&output), vec![30, 10]);
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
            write_selected_pages(input.clone(), vec![1], input.clone()),
            "Save the PDF as a new file, not over the input file.",
        );
        assert_error(
            write_selected_pages(input, vec![3], dir.join("missing.pdf")),
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
        let files = sorted_pdf_files(&dir.join("parts"));
        assert_eq!(files.len(), 3);
        assert_eq!(page_markers(&files[0]), vec![10, 20]);
        assert_eq!(page_markers(&files[1]), vec![30, 40]);
        assert_eq!(page_markers(&files[2]), vec![50]);
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
