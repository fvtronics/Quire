/* tests.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use super::{
    app_producer_metadata, compress_pdf_blocking, edit_pdf_metadata_blocking, load_document,
    merge_pdfs_blocking, parse_page_numbers, parse_page_ranges, split_breaks, split_pdf_blocking,
    watermark_pdf_blocking, write_selected_pages, CompressOptions, PageSelection, PdfBackendError,
    PdfDocumentMetadata, PdfEditableMetadata, PdfInput, PdfOutputOptions, PdfSaveOptions,
    SplitRule, WatermarkLayer, WatermarkOptions, WatermarkTarget,
};
use lopdf::{
    content::Content, dictionary, Document, EncryptionState, EncryptionVersion, Object,
    Permissions, Stream,
};
use std::fs;
use std::path::{Path, PathBuf};
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
        let directory_name = format!("quire-{name}-{}-{unique}", std::process::id());
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
    assert_eq!(
        parse_page_ranges("1-3,3,4,1,2", 10).unwrap(),
        vec![1, 2, 3, 4]
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
    assert_eq!(
        parse_page_numbers("1,2,2,3,4,5,5", 10).unwrap(),
        vec![1, 2, 3, 4, 5]
    );

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
        PdfOutputOptions::default(),
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
        merge_pdfs_blocking(
            vec![pdf_input(input.clone(), 0)],
            dir.join("out.pdf"),
            PdfOutputOptions::default(),
        ),
        "Choose at least two PDF files to merge.",
    );
    assert_error(
        merge_pdfs_blocking(
            vec![
                pdf_input(input.clone(), 0),
                pdf_input(dir.join("other.pdf"), 0),
            ],
            input,
            PdfOutputOptions::default(),
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
        PdfOutputOptions::default(),
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(page_rotations(&output), vec![90, 90, 180]);
}

#[test]
fn merge_pdfs_removes_metadata_from_all_inputs() {
    let dir = TestDir::new("merge-remove-metadata");
    let first = dir.join("first.pdf");
    let second = dir.join("second.pdf");
    let output = dir.join("merged.pdf");
    write_test_pdf(&first, &[10]);
    write_test_pdf(&second, &[20]);
    add_test_metadata(&first);
    add_test_metadata(&second);

    let result = merge_pdfs_blocking(
        vec![pdf_input(first, 0), pdf_input(second, 0)],
        output.clone(),
        PdfOutputOptions {
            save: PdfSaveOptions {
                remove_metadata: true,
                ..Default::default()
            },
            ..Default::default()
        },
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(page_markers(&output), vec![10, 20]);
    assert!(!has_metadata(&output));
    assert!(!contains_private_metadata(&output));
}

#[test]
fn write_selected_pages_uses_requested_order() {
    let dir = TestDir::new("selected-pages");
    let input = dir.join("input.pdf");
    let output = dir.join("output.pdf");
    write_test_pdf(&input, &[10, 20, 30]);

    let result = write_selected_pages(
        input.clone(),
        None,
        vec![page_selection(3, 0), page_selection(1, 0)],
        output.clone(),
        PdfOutputOptions::default(),
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(page_markers(&output), vec![30, 10]);
}

#[test]
fn write_selected_pages_duplicates_and_inserts_blank_pages() {
    let dir = TestDir::new("selected-pages-duplicate-blank");
    let input = dir.join("input.pdf");
    let output = dir.join("output.pdf");
    write_test_pdf(&input, &[10, 20]);

    let result = write_selected_pages(
        input.clone(),
        None,
        vec![
            page_selection(1, 90),
            page_selection(1, 180),
            PageSelection::blank_like_page(2, 0),
        ],
        output.clone(),
        PdfOutputOptions::default(),
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(page_markers(&output), vec![10, 10, 20]);
    assert_eq!(page_rotations(&output), vec![90, 180, 0]);
    assert_eq!(page_has_contents(&output), vec![true, true, false]);
}

#[test]
fn write_selected_pages_rotates_requested_pages() {
    let dir = TestDir::new("selected-pages-rotation");
    let input = dir.join("input.pdf");
    let output = dir.join("output.pdf");
    write_test_pdf(&input, &[10, 20, 30]);

    let result = write_selected_pages(
        input.clone(),
        None,
        vec![page_selection(3, 90), page_selection(1, 270)],
        output.clone(),
        PdfOutputOptions::default(),
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(page_markers(&output), vec![30, 10]);
    assert_eq!(page_rotations(&output), vec![90, 270]);
}

#[test]
fn write_selected_pages_normalizes_page_boxes() {
    let dir = TestDir::new("selected-pages-options");
    let input = dir.join("input.pdf");
    let output = dir.join("output.pdf");
    write_test_pdf(&input, &[10, 20, 30]);

    let result = write_selected_pages(
        input.clone(),
        None,
        vec![page_selection(2, 0), page_selection(3, 0)],
        output.clone(),
        PdfOutputOptions {
            normalize_page_size: true,
            ..Default::default()
        },
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(page_markers(&output), vec![30, 30]);
    assert_eq!(page_boxes(&output), vec![[0, 0, 30, 100], [0, 0, 30, 100]]);
    assert_eq!(crop_boxes(&output), vec![[0, 0, 30, 100], [0, 0, 30, 100]]);
}

#[test]
fn write_selected_pages_normalizes_rotated_display_size() {
    let dir = TestDir::new("selected-pages-normalize-rotation");
    let input = dir.join("input.pdf");
    let output = dir.join("output.pdf");
    write_test_pdf(&input, &[20, 30]);

    let result = write_selected_pages(
        input.clone(),
        None,
        vec![page_selection(1, 0), page_selection(2, 90)],
        output.clone(),
        PdfOutputOptions {
            normalize_page_size: true,
            ..Default::default()
        },
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(page_rotations(&output), vec![0, 90]);
    assert_eq!(
        page_boxes(&output),
        vec![[0, 0, 100, 100], [0, 0, 100, 100]]
    );
    assert_eq!(
        crop_boxes(&output),
        vec![[0, 0, 100, 100], [0, 0, 100, 100]]
    );
}

#[test]
fn write_selected_pages_removes_metadata() {
    let dir = TestDir::new("selected-pages-remove-metadata");
    let input = dir.join("input.pdf");
    let output = dir.join("output.pdf");
    write_test_pdf(&input, &[10, 20]);
    add_test_metadata(&input);

    let result = write_selected_pages(
        input.clone(),
        None,
        vec![page_selection(1, 0), page_selection(2, 0)],
        output.clone(),
        PdfOutputOptions {
            save: PdfSaveOptions {
                remove_metadata: true,
                ..Default::default()
            },
            ..Default::default()
        },
    );

    assert_eq!(result.unwrap(), output);
    assert!(!has_metadata(&output));
    assert!(!contains_private_metadata(&output));
}

#[test]
fn write_selected_pages_rejects_empty_pages_and_input_overwrite() {
    let dir = TestDir::new("selected-pages-invalid");
    let input = dir.join("input.pdf");
    write_test_pdf(&input, &[10, 20]);

    assert_error(
        write_selected_pages(
            input.clone(),
            None,
            Vec::new(),
            dir.join("empty.pdf"),
            PdfOutputOptions::default(),
        ),
        "Choose at least one page to save.",
    );
    assert_error(
        write_selected_pages(
            input.clone(),
            None,
            vec![page_selection(1, 0)],
            input.clone(),
            PdfOutputOptions::default(),
        ),
        "Save the PDF as a new file, not over the input file.",
    );
    assert_error(
        write_selected_pages(
            input,
            None,
            vec![page_selection(3, 0)],
            dir.join("missing.pdf"),
            PdfOutputOptions::default(),
        ),
        "Page 3 is not in this PDF.",
    );
}

#[test]
fn load_document_reports_missing_and_invalid_passwords() {
    let dir = TestDir::new("encrypted-load");
    let input = dir.join("locked.pdf");
    write_encrypted_test_pdf(&input, &[10], "secret");

    let missing_password = load_document(&input, None).unwrap_err();
    assert!(matches!(
        missing_password,
        PdfBackendError::PasswordRequired { .. }
    ));
    assert_eq!(
        missing_password.to_string(),
        "locked.pdf is password protected."
    );

    let invalid_password = load_document(&input, Some("wrong")).unwrap_err();
    assert!(matches!(
        invalid_password,
        PdfBackendError::InvalidPassword { .. }
    ));
    assert_eq!(
        invalid_password.to_string(),
        "The password for locked.pdf is incorrect."
    );
}

#[test]
fn write_selected_pages_accepts_valid_password() {
    let dir = TestDir::new("encrypted-selected-pages");
    let input = dir.join("locked.pdf");
    let output = dir.join("unlocked-output.pdf");
    write_encrypted_test_pdf(&input, &[10, 20], "secret");

    let result = write_selected_pages(
        input,
        Some("secret".to_string()),
        vec![page_selection(2, 0)],
        output.clone(),
        PdfOutputOptions::default(),
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(page_markers(&output), vec![20]);
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
        None,
        output_folder.clone(),
        "Chapter".to_string(),
        SplitRule::EveryNPages(2),
        PdfOutputOptions::default(),
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
        None,
        output_folder.clone(),
        "Page".to_string(),
        SplitRule::EveryPage,
        PdfOutputOptions::default(),
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
        None,
        output_folder.clone(),
        "Even".to_string(),
        SplitRule::EvenPages,
        PdfOutputOptions::default(),
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
        None,
        output_folder.clone(),
        "Odd".to_string(),
        SplitRule::OddPages,
        PdfOutputOptions::default(),
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
        None,
        output_folder.clone(),
        "Specific".to_string(),
        SplitRule::SpecificPages(vec![4, 2, 2]),
        PdfOutputOptions::default(),
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
        None,
        output_folder.clone(),
        "   ".to_string(),
        SplitRule::EveryPage,
        PdfOutputOptions::default(),
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
            None,
            output_folder,
            "Empty".to_string(),
            SplitRule::EveryPage,
            PdfOutputOptions::default(),
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
        None,
        output_folder,
        "Broken".to_string(),
        SplitRule::EveryPage,
        PdfOutputOptions::default(),
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
        None,
        missing_output_folder,
        "Split".to_string(),
        SplitRule::EveryPage,
        PdfOutputOptions::default(),
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
        None,
        output.clone(),
        CompressOptions {
            remove_empty_streams: true,
            prune_objects: true,
            save: PdfSaveOptions::default(),
        },
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(page_markers(&output), vec![10, 20]);
    assert_does_not_use_object_streams(&output);
    assert_error(
        compress_pdf_blocking(
            input.clone(),
            None,
            input,
            CompressOptions {
                remove_empty_streams: false,
                prune_objects: false,
                save: PdfSaveOptions::default(),
            },
        ),
        "Save the PDF as a new file, not over the input file.",
    );
}

#[test]
fn edit_pdf_metadata_writes_document_info_fields() {
    let dir = TestDir::new("metadata-edit");
    let input = dir.join("input.pdf");
    let output = dir.join("output.pdf");
    write_test_pdf(&input, &[10, 20]);

    edit_pdf_metadata_blocking(
        input.clone(),
        None,
        output.clone(),
        PdfEditableMetadata {
            title: "Edited Title".to_string(),
            author: "Edited Author".to_string(),
            subject: "Edited Subject".to_string(),
            keywords: "alpha, beta".to_string(),
        },
        PdfSaveOptions::default(),
    )
    .unwrap();

    assert_eq!(page_markers(&output), vec![10, 20]);
    assert!(uses_incremental_update(&output));
    assert!(fs::metadata(&output).unwrap().len() > fs::metadata(&input).unwrap().len());
    assert_eq!(metadata_field(&output, b"Title"), "Edited Title");
    assert_eq!(metadata_field(&output, b"Author"), "Edited Author");
    assert_eq!(metadata_field(&output, b"Subject"), "Edited Subject");
    assert_eq!(metadata_field(&output, b"Keywords"), "alpha, beta");
    assert!(!has_metadata_field(&output, b"Creator"));
    assert_eq!(
        metadata_field(&output, b"Producer"),
        app_producer_metadata()
    );
}

#[test]
fn edit_pdf_metadata_removes_empty_known_fields() {
    let dir = TestDir::new("metadata-empty");
    let input = dir.join("input.pdf");
    let output = dir.join("output.pdf");
    write_test_pdf(&input, &[1]);
    set_test_info_metadata(
        &input,
        PdfDocumentMetadata {
            title: "Old Title".to_string(),
            author: "Old Author".to_string(),
            creator: "Original Creator".to_string(),
            ..Default::default()
        },
    );

    edit_pdf_metadata_blocking(
        input,
        None,
        output.clone(),
        PdfEditableMetadata {
            title: "New Title".to_string(),
            ..Default::default()
        },
        PdfSaveOptions::default(),
    )
    .unwrap();

    assert_eq!(metadata_field(&output, b"Title"), "New Title");
    assert!(!has_metadata_field(&output, b"Author"));
    assert_eq!(metadata_field(&output, b"Creator"), "Original Creator");
}

#[test]
fn edit_pdf_metadata_rejects_input_overwrite() {
    let dir = TestDir::new("metadata-overwrite");
    let input = dir.join("input.pdf");
    write_test_pdf(&input, &[1]);

    let result = edit_pdf_metadata_blocking(
        input.clone(),
        None,
        input,
        PdfEditableMetadata::default(),
        PdfSaveOptions::default(),
    );

    assert_error(
        result,
        "Save the PDF as a new file, not over the input file.",
    );
}

#[test]
fn edit_pdf_metadata_remove_metadata_uses_full_clean_save() {
    let dir = TestDir::new("metadata-clean");
    let input = dir.join("input.pdf");
    let output = dir.join("output.pdf");
    write_test_pdf(&input, &[1]);
    add_test_metadata(&input);

    edit_pdf_metadata_blocking(
        input,
        None,
        output.clone(),
        PdfEditableMetadata {
            title: "Public Title".to_string(),
            ..Default::default()
        },
        PdfSaveOptions {
            remove_metadata: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(!uses_incremental_update(&output));
    assert!(!contains_private_metadata(&output));
    assert_eq!(metadata_field(&output, b"Title"), "Public Title");
    assert_eq!(
        metadata_field(&output, b"Producer"),
        app_producer_metadata()
    );
}

#[test]
fn edit_pdf_metadata_modern_pdf_uses_full_modern_save() {
    let dir = TestDir::new("metadata-modern");
    let input = dir.join("input.pdf");
    let output = dir.join("output.pdf");
    write_test_pdf(&input, &[1]);

    edit_pdf_metadata_blocking(
        input,
        None,
        output.clone(),
        PdfEditableMetadata {
            title: "Modern Title".to_string(),
            ..Default::default()
        },
        PdfSaveOptions {
            modern_pdf: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_uses_modern_pdf(&output);
    assert_eq!(metadata_field(&output, b"Title"), "Modern Title");
}

#[test]
fn watermark_pdf_adds_foreground_stream_to_all_pages() {
    let dir = TestDir::new("watermark-all");
    let input = dir.join("input.pdf");
    let image = dir.join("watermark.png");
    let output = dir.join("watermarked.pdf");
    write_test_pdf(&input, &[10, 20]);
    write_test_png(&image);

    let result = watermark_pdf_blocking(
        input,
        None,
        output.clone(),
        watermark_options(image, WatermarkLayer::Foreground, WatermarkTarget::AllPages),
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(watermarked_pages(&output), vec![1, 2]);
    assert_eq!(page_content_counts(&output), vec![2, 2]);
    assert!(page_watermark_is_last(&output, 1));
    assert!(page_watermark_is_last(&output, 2));
    assert!(first_page_has_watermark_xobject(&output));
}

#[test]
fn watermark_pdf_adds_background_stream_before_existing_contents() {
    let dir = TestDir::new("watermark-background");
    let input = dir.join("input.pdf");
    let image = dir.join("watermark.png");
    let output = dir.join("watermarked.pdf");
    write_test_pdf(&input, &[10, 20]);
    write_test_png(&image);

    let result = watermark_pdf_blocking(
        input,
        None,
        output.clone(),
        watermark_options(
            image,
            WatermarkLayer::Background,
            WatermarkTarget::FirstPage,
        ),
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(watermarked_pages(&output), vec![1]);
    assert_eq!(page_content_counts(&output), vec![2, 1]);
    assert!(page_watermark_is_first(&output, 1));
}

#[test]
fn watermark_pdf_targets_last_page() {
    let dir = TestDir::new("watermark-last");
    let input = dir.join("input.pdf");
    let image = dir.join("watermark.png");
    let output = dir.join("watermarked.pdf");
    write_test_pdf(&input, &[10, 20, 30]);
    write_test_png(&image);

    let result = watermark_pdf_blocking(
        input,
        None,
        output.clone(),
        watermark_options(image, WatermarkLayer::Foreground, WatermarkTarget::LastPage),
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(watermarked_pages(&output), vec![3]);
    assert_eq!(page_content_counts(&output), vec![1, 1, 2]);
}

#[test]
fn watermark_pdf_targets_specific_pages() {
    let dir = TestDir::new("watermark-specific");
    let input = dir.join("input.pdf");
    let image = dir.join("watermark.png");
    let output = dir.join("watermarked.pdf");
    write_test_pdf(&input, &[10, 20, 30, 40]);
    write_test_png(&image);

    let result = watermark_pdf_blocking(
        input,
        None,
        output.clone(),
        watermark_options(
            image,
            WatermarkLayer::Foreground,
            WatermarkTarget::SpecificPages(vec![2, 4]),
        ),
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(watermarked_pages(&output), vec![2, 4]);
    assert_eq!(page_content_counts(&output), vec![1, 2, 1, 2]);
}

#[test]
fn watermark_pdf_normalizes_specific_pages() {
    let dir = TestDir::new("watermark-specific-normalized");
    let input = dir.join("input.pdf");
    let image = dir.join("watermark.png");
    let output = dir.join("watermarked.pdf");
    write_test_pdf(&input, &[10, 20, 30, 40]);
    write_test_png(&image);

    let result = watermark_pdf_blocking(
        input,
        None,
        output.clone(),
        watermark_options(
            image,
            WatermarkLayer::Foreground,
            WatermarkTarget::SpecificPages(vec![4, 2, 4, 2]),
        ),
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(watermarked_pages(&output), vec![2, 4]);
    assert_eq!(page_content_counts(&output), vec![1, 2, 1, 2]);
}

#[test]
fn watermark_pdf_sets_opacity_graphics_state() {
    let dir = TestDir::new("watermark-opacity");
    let input = dir.join("input.pdf");
    let image = dir.join("watermark.png");
    let output = dir.join("watermarked.pdf");
    write_test_pdf(&input, &[10]);
    write_test_png(&image);

    let result = watermark_pdf_blocking(
        input,
        None,
        output.clone(),
        watermark_options_with_opacity(
            image,
            WatermarkLayer::Foreground,
            WatermarkTarget::AllPages,
            0.45,
        ),
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(first_page_watermark_opacity(&output), Some((0.45, 0.45)));
}

#[test]
fn watermark_pdf_flattens_indirect_contents_array() {
    let dir = TestDir::new("watermark-indirect-contents");
    let input = dir.join("input.pdf");
    let image = dir.join("watermark.png");
    let output = dir.join("watermarked.pdf");
    write_test_pdf_with_indirect_contents_array(&input);
    write_test_png(&image);

    let result = watermark_pdf_blocking(
        input,
        None,
        output.clone(),
        watermark_options(image, WatermarkLayer::Foreground, WatermarkTarget::AllPages),
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(page_content_counts(&output), vec![3]);
    assert!(page_watermark_is_last(&output, 1));
    assert!(first_page_contents_are_flat(&output));
}

#[test]
fn watermark_pdf_preserves_inherited_xobjects() {
    let dir = TestDir::new("watermark-inherited-resources");
    let input = dir.join("input.pdf");
    let image = dir.join("watermark.png");
    let output = dir.join("watermarked.pdf");
    write_test_pdf_with_inherited_xobject(&input);
    write_test_png(&image);

    let result = watermark_pdf_blocking(
        input,
        None,
        output.clone(),
        watermark_options(image, WatermarkLayer::Foreground, WatermarkTarget::AllPages),
    );

    assert_eq!(result.unwrap(), output);
    assert!(first_page_has_xobject(&output, b"ExistingImage"));
    assert!(first_page_has_xobject(&output, b"QuireWatermark1"));
}

#[test]
fn watermark_pdf_centers_inside_visible_crop_box() {
    let dir = TestDir::new("watermark-visible-box");
    let input = dir.join("input.pdf");
    let image = dir.join("watermark.png");
    let output = dir.join("watermarked.pdf");
    write_test_pdf_with_visible_crop_box(&input);
    write_test_png(&image);

    let result = watermark_pdf_blocking(
        input,
        None,
        output.clone(),
        watermark_options(image, WatermarkLayer::Foreground, WatermarkTarget::AllPages),
    );

    assert_eq!(result.unwrap(), output);
    assert_eq!(
        first_page_watermark_transform(&output),
        Some([84.0, 0.0, 0.0, 84.0, 38.0, 108.0])
    );
}

#[test]
fn watermark_pdf_rejects_input_overwrite_and_invalid_specific_pages() {
    let dir = TestDir::new("watermark-invalid");
    let input = dir.join("input.pdf");
    let image = dir.join("watermark.png");
    let output = dir.join("watermarked.pdf");
    write_test_pdf(&input, &[10, 20]);
    write_test_png(&image);

    assert_error(
        watermark_pdf_blocking(
            input.clone(),
            None,
            input.clone(),
            watermark_options(
                image.clone(),
                WatermarkLayer::Foreground,
                WatermarkTarget::AllPages,
            ),
        ),
        "Save the PDF as a new file, not over the input file.",
    );
    assert_error(
        watermark_pdf_blocking(
            input,
            None,
            output,
            watermark_options(
                image,
                WatermarkLayer::Foreground,
                WatermarkTarget::SpecificPages(vec![3]),
            ),
        ),
        "Page 3 is not in this PDF.",
    );
}

#[test]
fn save_options_control_modern_pdf_output() {
    let dir = TestDir::new("save-options");
    let first = dir.join("first.pdf");
    let second = dir.join("second.pdf");
    let modern_output = dir.join("modern.pdf");
    let traditional_output = dir.join("traditional.pdf");
    write_test_pdf(&first, &[10]);
    write_test_pdf(&second, &[20]);

    merge_pdfs_blocking(
        vec![pdf_input(first.clone(), 0), pdf_input(second.clone(), 0)],
        modern_output.clone(),
        PdfOutputOptions {
            save: PdfSaveOptions {
                modern_pdf: true,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    merge_pdfs_blocking(
        vec![pdf_input(first, 0), pdf_input(second, 0)],
        traditional_output.clone(),
        PdfOutputOptions {
            save: PdfSaveOptions {
                modern_pdf: false,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(page_markers(&modern_output), vec![10, 20]);
    assert_uses_modern_pdf(&modern_output);
    assert_eq!(page_markers(&traditional_output), vec![10, 20]);
    assert_does_not_use_object_streams(&traditional_output);
}

#[test]
fn split_pdf_applies_save_options_to_each_output() {
    let dir = TestDir::new("split-save-options");
    let input = dir.join("input.pdf");
    let modern_folder = dir.join("modern");
    let traditional_folder = dir.join("traditional");
    fs::create_dir(&modern_folder).expect("modern output folder should be created");
    fs::create_dir(&traditional_folder).expect("traditional output folder should be created");
    write_test_pdf(&input, &[10, 20]);

    split_pdf_blocking(
        input.clone(),
        None,
        modern_folder.clone(),
        "Modern".to_string(),
        SplitRule::EveryPage,
        PdfOutputOptions {
            save: PdfSaveOptions {
                modern_pdf: true,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    split_pdf_blocking(
        input,
        None,
        traditional_folder.clone(),
        "Traditional".to_string(),
        SplitRule::EveryPage,
        PdfOutputOptions {
            save: PdfSaveOptions {
                modern_pdf: false,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();

    for path in sorted_pdf_files(&modern_folder) {
        assert_uses_modern_pdf(&path);
    }
    for path in sorted_pdf_files(&traditional_folder) {
        assert_does_not_use_object_streams(&path);
    }
}

fn assert_error<T: std::fmt::Debug>(result: Result<T, PdfBackendError>, message: &str) {
    assert_eq!(result.unwrap_err().to_string(), message);
}

fn assert_uses_modern_pdf(path: &Path) {
    let bytes = fs::read(path).expect("PDF output should be readable");
    assert!(
        contains_bytes(&bytes, b"/ObjStm"),
        "modern PDF output should include object streams"
    );
    assert!(
        contains_bytes(&bytes, b"/XRef"),
        "modern PDF output should include a cross-reference stream"
    );
}

fn assert_does_not_use_object_streams(path: &Path) {
    let bytes = fs::read(path).expect("PDF output should be readable");
    assert!(
        !contains_bytes(&bytes, b"/ObjStm"),
        "normal save output should not include generated object streams"
    );
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn uses_incremental_update(path: &Path) -> bool {
    let bytes = fs::read(path).expect("PDF output should be readable");
    bytes
        .windows(b"%%EOF".len())
        .filter(|window| *window == b"%%EOF")
        .count()
        > 1
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
    document.trailer.set(
        "ID",
        Object::Array(vec![
            Object::string_literal(b"quire-test-id-1"),
            Object::string_literal(b"quire-test-id-2"),
        ]),
    );
    document.compress();
    document.save(path).expect("test PDF should be saved");
}

fn write_test_pdf_with_indirect_contents_array(path: &Path) {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let first_content_id = document.add_object(Stream::new(dictionary! {}, b"q Q".to_vec()));
    let second_content_id = document.add_object(Stream::new(dictionary! {}, b"q Q".to_vec()));
    let contents_id = document.add_object(vec![
        Object::Reference(first_content_id),
        Object::Reference(second_content_id),
    ]);
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => contents_id,
        "Resources" => dictionary! {},
        "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
    });

    save_test_pdf(document, pages_id, vec![page_id.into()], path);
}

fn write_test_pdf_with_inherited_xobject(path: &Path) {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let existing_image_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 1,
            "Height" => 1,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
        },
        vec![0, 0, 0],
    ));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
    });

    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
            "Resources" => dictionary! {
                "XObject" => dictionary! {
                    "ExistingImage" => existing_image_id,
                },
            },
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document.save(path).expect("test PDF should be saved");
}

fn write_test_pdf_with_visible_crop_box(path: &Path) {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
        "Resources" => dictionary! {},
        "MediaBox" => vec![10.into(), 20.into(), 210.into(), 320.into()],
        "CropBox" => vec![30.into(), 50.into(), 130.into(), 250.into()],
    });

    save_test_pdf(document, pages_id, vec![page_id.into()], path);
}

fn save_test_pdf(
    mut document: Document,
    pages_id: lopdf::ObjectId,
    kids: Vec<Object>,
    path: &Path,
) {
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids.clone(),
            "Count" => kids.len() as i64,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document.save(path).expect("test PDF should be saved");
}

fn write_encrypted_test_pdf(path: &Path, page_markers: &[i64], password: &str) {
    write_test_pdf(path, page_markers);

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

fn write_test_png(path: &Path) {
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 4, 4)
        .expect("test image surface should be created");
    let context = cairo::Context::new(&surface).expect("test image context should be created");
    context.set_source_rgba(0.0, 0.0, 0.0, 0.0);
    context.paint().expect("test image should be cleared");
    context.set_source_rgba(0.1, 0.4, 0.8, 0.5);
    context.rectangle(0.0, 0.0, 4.0, 4.0);
    context.fill().expect("test image should be painted");
    surface.flush();

    let mut file = fs::File::create(path).expect("test image should be created");
    surface
        .write_to_png(&mut file)
        .expect("test image should be saved");
}

fn add_test_metadata(path: &Path) {
    let mut document = Document::load(path).expect("test PDF should load");
    let info_id = document.add_object(dictionary! {
        "Title" => Object::string_literal("Private title"),
    });
    let metadata_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "Metadata",
            "Subtype" => "XML",
        },
        b"<metadata>private</metadata>".to_vec(),
    ));

    document.trailer.set("Info", info_id);
    document
        .catalog_mut()
        .expect("test PDF should have a catalog")
        .set("Metadata", metadata_id);
    document.save(path).expect("test PDF should be saved");
}

fn set_test_info_metadata(path: &Path, metadata: PdfDocumentMetadata) {
    let mut document = Document::load(path).expect("test PDF should load");
    let info_id = document.add_object(dictionary! {
        "Title" => lopdf::text_string(&metadata.title),
        "Author" => lopdf::text_string(&metadata.author),
        "Subject" => lopdf::text_string(&metadata.subject),
        "Keywords" => lopdf::text_string(&metadata.keywords),
        "Creator" => lopdf::text_string(&metadata.creator),
    });
    document.trailer.set("Info", info_id);
    document.save(path).expect("test PDF should be saved");
}

fn pdf_input(path: PathBuf, rotation: i64) -> PdfInput {
    PdfInput {
        path,
        password: None,
        rotation,
    }
}

fn page_selection(page_number: u32, rotation: i64) -> PageSelection {
    PageSelection::page(page_number, rotation)
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

fn page_has_contents(path: &Path) -> Vec<bool> {
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
                .get(b"Contents")
                .is_ok()
        })
        .collect()
}

fn page_content_counts(path: &Path) -> Vec<usize> {
    let document = Document::load(path).expect("test PDF should load");
    document
        .get_pages()
        .into_values()
        .map(|object_id| document.get_page_contents(object_id).len())
        .collect()
}

fn watermarked_pages(path: &Path) -> Vec<u32> {
    let document = Document::load(path).expect("test PDF should load");
    document
        .get_pages()
        .into_iter()
        .filter_map(|(page_number, object_id)| {
            page_contains_watermark(&document, object_id).then_some(page_number)
        })
        .collect()
}

fn first_page_has_watermark_xobject(path: &Path) -> bool {
    first_page_has_xobject(path, b"QuireWatermark1")
}

fn first_page_has_xobject(path: &Path, name: &[u8]) -> bool {
    let document = Document::load(path).expect("test PDF should load");
    let page_id = document
        .get_pages()
        .into_values()
        .next()
        .expect("test PDF should have a first page");
    let page = document
        .get_object(page_id)
        .expect("page object should exist")
        .as_dict()
        .expect("page should be a dictionary");
    let resources = page
        .get(b"Resources")
        .expect("watermarked page should have resources")
        .as_dict()
        .expect("resources should be a dictionary");
    let xobjects = resources
        .get(b"XObject")
        .expect("resources should have xobjects")
        .as_dict()
        .expect("xobjects should be a dictionary");

    xobjects.has(name)
}

fn first_page_watermark_opacity(path: &Path) -> Option<(f32, f32)> {
    let document = Document::load(path).expect("test PDF should load");
    let page_id = document.get_pages().into_values().next()?;
    let page = document.get_object(page_id).ok()?.as_dict().ok()?;
    let resources = page.get(b"Resources").ok()?.as_dict().ok()?;
    let ext_gstates = resources.get(b"ExtGState").ok()?.as_dict().ok()?;
    let opacity = ext_gstates
        .get(b"QuireWatermarkOpacity1")
        .ok()?
        .as_dict()
        .ok()?;
    let stroke = opacity.get(b"CA").ok()?.as_float().ok()?;
    let non_stroke = opacity.get(b"ca").ok()?.as_float().ok()?;

    Some((stroke, non_stroke))
}

fn page_contains_watermark(document: &Document, page_id: lopdf::ObjectId) -> bool {
    document
        .get_page_contents(page_id)
        .iter()
        .any(|content_id| content_stream_contains(document, *content_id, b"QuireWatermark"))
}

fn page_watermark_is_first(path: &Path, page_number: u32) -> bool {
    page_watermark_position(path, page_number).is_some_and(|position| position == 0)
}

fn page_watermark_is_last(path: &Path, page_number: u32) -> bool {
    let document = Document::load(path).expect("test PDF should load");
    let page_id = document
        .get_pages()
        .get(&page_number)
        .copied()
        .expect("test page should exist");
    let contents = document.get_page_contents(page_id);

    page_watermark_position(path, page_number)
        .is_some_and(|position| position + 1 == contents.len())
}

fn page_watermark_position(path: &Path, page_number: u32) -> Option<usize> {
    let document = Document::load(path).expect("test PDF should load");
    let page_id = document
        .get_pages()
        .get(&page_number)
        .copied()
        .expect("test page should exist");

    document
        .get_page_contents(page_id)
        .iter()
        .position(|content_id| content_stream_contains(&document, *content_id, b"QuireWatermark"))
}

fn first_page_contents_are_flat(path: &Path) -> bool {
    let document = Document::load(path).expect("test PDF should load");
    let Some(page_id) = document.get_pages().into_values().next() else {
        return false;
    };
    let Ok(page) = document.get_object(page_id).and_then(Object::as_dict) else {
        return false;
    };
    let Ok(contents) = page.get(b"Contents").and_then(Object::as_array) else {
        return false;
    };

    contents
        .iter()
        .all(|content| !matches!(content, Object::Array(_)))
}

fn first_page_watermark_transform(path: &Path) -> Option<[f32; 6]> {
    let document = Document::load(path).expect("test PDF should load");
    let page_id = document.get_pages().into_values().next()?;

    page_content_ids(&document, page_id)
        .into_iter()
        .filter_map(|content_id| content_stream_transform(&document, content_id))
        .next()
}

fn page_content_ids(document: &Document, page_id: lopdf::ObjectId) -> Vec<lopdf::ObjectId> {
    let Some(contents) = document
        .get_object(page_id)
        .ok()
        .and_then(|page| page.as_dict().ok())
        .and_then(|page| page.get(b"Contents").ok())
    else {
        return Vec::new();
    };

    match contents {
        Object::Array(contents) => contents
            .iter()
            .filter_map(|content| content.as_reference().ok())
            .collect(),
        Object::Reference(object_id) => vec![*object_id],
        _ => Vec::new(),
    }
}

fn content_stream_transform(document: &Document, object_id: lopdf::ObjectId) -> Option<[f32; 6]> {
    let stream = document.get_object(object_id).ok()?.as_stream().ok()?;
    let content = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    let content = Content::decode(&content).ok()?;
    let transform = content
        .operations
        .iter()
        .find(|operation| operation.operator == "cm")?;

    Some([
        transform.operands.first()?.as_float().ok()?,
        transform.operands.get(1)?.as_float().ok()?,
        transform.operands.get(2)?.as_float().ok()?,
        transform.operands.get(3)?.as_float().ok()?,
        transform.operands.get(4)?.as_float().ok()?,
        transform.operands.get(5)?.as_float().ok()?,
    ])
}

fn content_stream_contains(document: &Document, object_id: lopdf::ObjectId, needle: &[u8]) -> bool {
    document
        .get_object(object_id)
        .and_then(Object::as_stream)
        .ok()
        .map(|stream| {
            stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone())
        })
        .is_some_and(|content| content.windows(needle.len()).any(|window| window == needle))
}

fn watermark_options(
    image_file: PathBuf,
    layer: WatermarkLayer,
    target: WatermarkTarget,
) -> WatermarkOptions {
    watermark_options_with_opacity(image_file, layer, target, 1.0)
}

fn watermark_options_with_opacity(
    image_file: PathBuf,
    layer: WatermarkLayer,
    target: WatermarkTarget,
    opacity: f32,
) -> WatermarkOptions {
    WatermarkOptions {
        image_file,
        layer,
        target,
        opacity,
        save: PdfSaveOptions::default(),
    }
}

fn metadata_field(path: &Path, key: &[u8]) -> String {
    let document = Document::load(path).expect("test PDF should load");
    let info = document
        .trailer
        .get(b"Info")
        .expect("metadata should have an info dictionary");
    let (_, info) = document
        .dereference(info)
        .expect("info dictionary should dereference");
    let info = info.as_dict().expect("info should be a dictionary");
    lopdf::decode_text_string(info.get(key).expect("field should exist"))
        .expect("field should decode")
}

fn has_metadata_field(path: &Path, key: &[u8]) -> bool {
    let document = Document::load(path).expect("test PDF should load");
    let Ok(info) = document.trailer.get(b"Info") else {
        return false;
    };
    let Ok((_, info)) = document.dereference(info) else {
        return false;
    };
    info.as_dict()
        .is_ok_and(|dictionary| dictionary.get(key).is_ok())
}

fn page_boxes(path: &Path) -> Vec<[i64; 4]> {
    page_box_values(path, b"MediaBox")
}

fn crop_boxes(path: &Path) -> Vec<[i64; 4]> {
    page_box_values(path, b"CropBox")
}

fn page_box_values(path: &Path, key: &[u8]) -> Vec<[i64; 4]> {
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
                .get(key)
                .expect("page should have requested box")
                .as_array()
                .expect("page box should be an array");
            [
                media_box[0].as_i64().expect("left should be an integer"),
                media_box[1].as_i64().expect("bottom should be an integer"),
                media_box[2].as_i64().expect("right should be an integer"),
                media_box[3].as_i64().expect("top should be an integer"),
            ]
        })
        .collect()
}

fn has_metadata(path: &Path) -> bool {
    let document = Document::load(path).expect("test PDF should load");
    let has_info = document.trailer.get(b"Info").is_ok();
    let has_catalog_metadata = document
        .catalog()
        .expect("test PDF should have a catalog")
        .get(b"Metadata")
        .is_ok();
    let has_metadata_stream = document
        .objects
        .values()
        .any(|object| object.type_name().ok() == Some(b"Metadata"));

    has_info || has_catalog_metadata || has_metadata_stream
}

fn contains_private_metadata(path: &Path) -> bool {
    let bytes = fs::read(path).expect("test PDF should be readable");
    bytes
        .windows(b"private".len())
        .any(|window| window == b"private")
        || bytes
            .windows(b"Private title".len())
            .any(|window| window == b"Private title")
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
