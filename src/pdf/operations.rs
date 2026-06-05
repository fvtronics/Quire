/* operations.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use gtk::gio;
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::document::{
    build_and_save_document, load_document, remove_metadata, save_document, OutputPages,
};
use super::metadata::edit_pdf_metadata_blocking;
use super::pages::{
    blank_page_object, inherited_visible_page_box, page_selections, rotated_page_object,
};
use super::ranges::{split_breaks, split_output_prefix};
use super::types::{
    CompressOptions, PageSelection, PdfBackendError, PdfEditableMetadata, PdfInput,
    PdfOutputOptions, PdfSaveOptions, SplitRule, WatermarkLayer, WatermarkOptions, WatermarkTarget,
};

const WATERMARK_MARGIN_RATIO: f32 = 0.08;

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

pub async fn watermark_pdf(
    input_file: PathBuf,
    password: Option<String>,
    output_file: PathBuf,
    options: WatermarkOptions,
) -> Result<PathBuf, PdfBackendError> {
    gio::spawn_blocking(move || watermark_pdf_blocking(input_file, password, output_file, options))
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

    for (next_page_id, selection) in (document.max_id + 1..).zip(page_numbers) {
        let object_id = pages.get(&selection.page_number).ok_or_else(|| {
            PdfBackendError::InvalidPageRange(format!(
                "Page {} is not in this PDF.",
                selection.page_number
            ))
        })?;
        let page_id = (next_page_id, 0);
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

pub(super) fn watermark_pdf_blocking(
    input_file: PathBuf,
    password: Option<String>,
    output_file: PathBuf,
    options: WatermarkOptions,
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
    let pages = document.get_pages();
    let target_pages = watermark_target_pages(&options.target, pages.len())?;
    let image = WatermarkImage::load(&options.image_file)?;
    let image_id = insert_watermark_image(&mut document, image);

    for page_number in target_pages {
        let page_id = *pages.get(&page_number).ok_or_else(|| {
            PdfBackendError::InvalidPageRange(format!("Page {page_number} is not in this PDF."))
        })?;
        add_watermark_to_page(
            &mut document,
            page_id,
            image_id,
            options.layer,
            options.opacity,
        )?;
    }

    save_document(document, catalog_id, &output_file, options.save)
}

fn watermark_target_pages(
    target: &WatermarkTarget,
    page_count: usize,
) -> Result<Vec<u32>, PdfBackendError> {
    if page_count == 0 {
        return Err(PdfBackendError::NoPagesSelected);
    }

    match target {
        WatermarkTarget::AllPages => Ok((1..=page_count as u32).collect()),
        WatermarkTarget::FirstPage => Ok(vec![1]),
        WatermarkTarget::LastPage => Ok(vec![page_count as u32]),
        WatermarkTarget::SpecificPages(pages) => {
            if pages.is_empty() {
                return Err(PdfBackendError::NoPagesSelected);
            }

            let mut pages = pages.clone();
            for page in &pages {
                if *page == 0 || *page as usize > page_count {
                    return Err(PdfBackendError::InvalidPageRange(format!(
                        "Page {page} is not in this PDF."
                    )));
                }
            }

            pages.sort_unstable();
            pages.dedup();
            Ok(pages)
        }
    }
}

fn add_watermark_to_page(
    document: &mut Document,
    page_id: ObjectId,
    image_id: ObjectId,
    layer: WatermarkLayer,
    opacity: f32,
) -> Result<(), PdfBackendError> {
    let (page_x, page_y, page_width, page_height) = inherited_visible_page_box(document, page_id)?;
    let (image_width, image_height) = watermark_image_dimensions(document, image_id)?;
    let transform = centered_fit_transform(
        page_x,
        page_y,
        page_width,
        page_height,
        image_width,
        image_height,
    );
    let (image_name, opacity_name) = add_watermark_resources(document, page_id, image_id, opacity)?;
    let content_id = watermark_content_stream(document, &image_name, &opacity_name, transform)?;
    insert_page_content(document, page_id, content_id, layer)
}

fn watermark_image_dimensions(
    document: &Document,
    image_id: ObjectId,
) -> Result<(f32, f32), PdfBackendError> {
    let dictionary = document
        .get_object(image_id)
        .and_then(Object::as_stream)
        .map(|stream| &stream.dict)
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?;
    let width = dictionary
        .get(b"Width")
        .and_then(Object::as_i64)
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?
        as f32;
    let height = dictionary
        .get(b"Height")
        .and_then(Object::as_i64)
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?
        as f32;

    Ok((width, height))
}

fn centered_fit_transform(
    page_x: f32,
    page_y: f32,
    page_width: f32,
    page_height: f32,
    image_width: f32,
    image_height: f32,
) -> [f32; 6] {
    let max_width = page_width * (1.0 - WATERMARK_MARGIN_RATIO * 2.0);
    let max_height = page_height * (1.0 - WATERMARK_MARGIN_RATIO * 2.0);
    let scale = (max_width / image_width).min(max_height / image_height);
    let width = image_width * scale;
    let height = image_height * scale;
    let x = page_x + (page_width - width) / 2.0;
    let y = page_y + (page_height - height) / 2.0;

    [width, 0.0, 0.0, height, x, y]
}

fn add_watermark_resources(
    document: &mut Document,
    page_id: ObjectId,
    image_id: ObjectId,
    opacity: f32,
) -> Result<(String, String), PdfBackendError> {
    let mut resources = page_resources(document, page_id)?;
    let mut xobjects = resources
        .get(b"XObject")
        .ok()
        .and_then(|object| resource_dictionary(document, object))
        .unwrap_or_default();
    let image_name = unique_resource_name(&xobjects, "QuireWatermark");
    xobjects.set(image_name.as_str(), image_id);
    resources.set("XObject", xobjects);

    let mut ext_gstates = resources
        .get(b"ExtGState")
        .ok()
        .and_then(|object| resource_dictionary(document, object))
        .unwrap_or_default();
    let opacity_name = unique_resource_name(&ext_gstates, "QuireWatermarkOpacity");
    ext_gstates.set(
        opacity_name.as_str(),
        dictionary! {
            "Type" => "ExtGState",
            "CA" => opacity,
            "ca" => opacity,
        },
    );
    resources.set("ExtGState", ext_gstates);

    let page = document
        .get_object_mut(page_id)
        .and_then(Object::as_dict_mut)
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?;
    page.set("Resources", resources);

    Ok((image_name, opacity_name))
}

fn page_resources(document: &Document, page_id: ObjectId) -> Result<Dictionary, PdfBackendError> {
    let mut object_id = page_id;
    for _ in 0..document.objects.len() {
        let dictionary = document
            .get_object(object_id)
            .and_then(Object::as_dict)
            .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?;

        if let Ok(resources) = dictionary.get(b"Resources") {
            return Ok(resource_dictionary(document, resources).unwrap_or_default());
        }

        let Ok(parent_id) = dictionary.get(b"Parent").and_then(Object::as_reference) else {
            break;
        };
        object_id = parent_id;
    }

    Ok(Dictionary::new())
}

fn resource_dictionary(document: &Document, object: &Object) -> Option<Dictionary> {
    match object {
        Object::Dictionary(dictionary) => Some(dictionary.clone()),
        Object::Reference(object_id) => document.get_dictionary(*object_id).ok().cloned(),
        _ => None,
    }
}

fn unique_resource_name(resources: &Dictionary, prefix: &str) -> String {
    let mut index = 1;
    loop {
        let name = format!("{prefix}{index}");
        if !resources.has(name.as_bytes()) {
            return name;
        }
        index += 1;
    }
}

fn watermark_content_stream(
    document: &mut Document,
    resource_name: &str,
    opacity_name: &str,
    transform: [f32; 6],
) -> Result<ObjectId, PdfBackendError> {
    let operations = vec![
        Operation::new("q", Vec::new()),
        Operation::new("gs", vec![Object::Name(opacity_name.as_bytes().to_vec())]),
        Operation::new(
            "cm",
            transform
                .iter()
                .copied()
                .map(Object::Real)
                .collect::<Vec<_>>(),
        ),
        Operation::new("Do", vec![Object::Name(resource_name.as_bytes().to_vec())]),
        Operation::new("Q", Vec::new()),
    ];
    let content = Content { operations }
        .encode()
        .map_err(|error| PdfBackendError::Write(error.to_string()))?;
    let mut stream = Stream::new(dictionary! {}, content);
    let _ = stream.compress();

    Ok(document.add_object(stream))
}

fn insert_page_content(
    document: &mut Document,
    page_id: ObjectId,
    content_id: ObjectId,
    layer: WatermarkLayer,
) -> Result<(), PdfBackendError> {
    let contents = page_content_objects(document, page_id)?;
    let page = document
        .get_object_mut(page_id)
        .and_then(Object::as_dict_mut)
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?;
    let watermark = Object::Reference(content_id);
    let contents = match layer {
        WatermarkLayer::Background => std::iter::once(watermark)
            .chain(contents)
            .collect::<Vec<_>>(),
        WatermarkLayer::Foreground => contents
            .into_iter()
            .chain(std::iter::once(watermark))
            .collect::<Vec<_>>(),
    };
    page.set("Contents", contents);

    Ok(())
}

fn page_content_objects(
    document: &Document,
    page_id: ObjectId,
) -> Result<Vec<Object>, PdfBackendError> {
    let page = document
        .get_object(page_id)
        .and_then(Object::as_dict)
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?;

    match page.get(b"Contents") {
        Ok(contents) => Ok(content_objects(document, contents)),
        Err(_) => Ok(Vec::new()),
    }
}

fn content_objects(document: &Document, contents: &Object) -> Vec<Object> {
    match contents {
        Object::Array(contents) => contents.clone(),
        Object::Reference(object_id) => match document.get_object(*object_id) {
            Ok(Object::Array(contents)) => contents.clone(),
            _ => vec![contents.clone()],
        },
        _ => vec![contents.clone()],
    }
}

fn insert_watermark_image(document: &mut Document, image: WatermarkImage) -> ObjectId {
    let soft_mask_id = image.alpha.map(|alpha| {
        let mut mask = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => image.width,
                "Height" => image.height,
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
            },
            alpha,
        );
        let _ = mask.compress();
        document.add_object(mask)
    });

    let mut dictionary = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => image.width,
        "Height" => image.height,
        "ColorSpace" => "DeviceRGB",
        "BitsPerComponent" => 8,
    };
    if let Some(soft_mask_id) = soft_mask_id {
        dictionary.set("SMask", soft_mask_id);
    }

    let mut stream = Stream::new(dictionary, image.rgb);
    let _ = stream.compress();
    document.add_object(stream)
}

struct WatermarkImage {
    width: i64,
    height: i64,
    rgb: Vec<u8>,
    alpha: Option<Vec<u8>>,
}

impl WatermarkImage {
    fn load(path: &Path) -> Result<Self, PdfBackendError> {
        let pixbuf =
            crate::image::load_pixbuf(path).map_err(|error| PdfBackendError::ImageLoad {
                path: path.to_path_buf(),
                message: error,
            })?;
        if pixbuf.bits_per_sample() != 8 || pixbuf.width() <= 0 || pixbuf.height() <= 0 {
            return Err(PdfBackendError::ImageLoad {
                path: path.to_path_buf(),
                message: "unsupported image format".to_string(),
            });
        }

        let width = pixbuf.width();
        let height = pixbuf.height();
        let channels = pixbuf.n_channels();
        let rowstride = pixbuf.rowstride();
        let pixels = pixbuf.read_pixel_bytes();
        let pixels = pixels.as_ref();
        let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
        let mut alpha = pixbuf
            .has_alpha()
            .then(|| Vec::with_capacity(width as usize * height as usize));
        let mut has_transparency = false;

        for y in 0..height {
            let row_start = y as usize * rowstride as usize;
            for x in 0..width {
                let pixel_start = row_start + x as usize * channels as usize;
                rgb.extend_from_slice(&pixels[pixel_start..pixel_start + 3]);
                if let Some(alpha) = alpha.as_mut() {
                    let value = pixels[pixel_start + 3];
                    has_transparency |= value < u8::MAX;
                    alpha.push(value);
                }
            }
        }

        Ok(Self {
            width: width.into(),
            height: height.into(),
            rgb,
            alpha: alpha.filter(|_| has_transparency),
        })
    }
}
