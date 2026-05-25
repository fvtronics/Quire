/* ranges.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use std::path::Path;

use super::types::{PdfBackendError, SplitRule};

const PAGE_RANGE_HINT: &str = "Enter page ranges like 1,3-5,8.";
const PAGE_LIST_HINT: &str = "Enter pages like 2,4,7.";

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

pub(super) fn split_breaks(rule: SplitRule, page_count: u32) -> Result<Vec<u32>, PdfBackendError> {
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

    normalize_page_list(&mut breaks);
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

    normalize_page_list(&mut pages);
    Ok(pages)
}

fn normalize_page_list(pages: &mut Vec<u32>) {
    pages.sort_unstable();
    pages.dedup();
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

pub(super) fn split_output_prefix(input_file: &Path, prefix: &str) -> String {
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
