/* types.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct PdfInput {
    pub path: PathBuf,
    pub password: Option<String>,
    pub rotation: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageSelectionKind {
    Page,
    Blank,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageSelection {
    pub page_number: u32,
    pub rotation: i64,
    pub kind: PageSelectionKind,
}

impl PageSelection {
    pub fn page(page_number: u32, rotation: i64) -> Self {
        Self {
            page_number,
            rotation,
            kind: PageSelectionKind::Page,
        }
    }

    pub fn blank_like_page(page_number: u32, rotation: i64) -> Self {
        Self {
            page_number,
            rotation,
            kind: PageSelectionKind::Blank,
        }
    }

    pub fn is_blank(&self) -> bool {
        self.kind == PageSelectionKind::Blank
    }

    pub fn rotate_clockwise(&mut self) {
        self.rotation = (self.rotation + 90).rem_euclid(360);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PdfSaveOptions {
    pub remove_metadata: bool,
    pub modern_pdf: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PdfOutputOptions {
    pub normalize_page_size: bool,
    pub save: PdfSaveOptions,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PdfDocumentMetadata {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
    pub creator: String,
    pub producer: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PdfEditableMetadata {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
}

#[derive(Debug)]
pub enum PdfBackendError {
    NotEnoughInputs,
    NoPagesSelected,
    OutputMatchesInput,
    Load { path: PathBuf, message: String },
    PasswordRequired { path: PathBuf },
    InvalidPassword { path: PathBuf },
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
    pub save: PdfSaveOptions,
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
            Self::PasswordRequired { path } => write!(
                f,
                "{} is password protected.",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("PDF")
            ),
            Self::InvalidPassword { path } => write!(
                f,
                "The password for {} is incorrect.",
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
