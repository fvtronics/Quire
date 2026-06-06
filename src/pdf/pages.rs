/* pages.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use lopdf::{Document, Object, ObjectId, dictionary};

use super::types::{PageSelection, PdfBackendError};

pub(super) fn page_selections(pages: std::ops::RangeInclusive<u32>) -> Vec<PageSelection> {
    pages
        .map(|page_number| PageSelection::page(page_number, 0))
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

pub(super) fn rotated_page_object(
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

pub(super) fn blank_page_object(
    document: &Document,
    source_object_id: ObjectId,
    rotation: i64,
) -> Result<Object, PdfBackendError> {
    let mut dictionary = dictionary! {
        "Type" => "Page",
        "Resources" => dictionary! {},
        "MediaBox" => inherited_page_object(document, source_object_id, b"MediaBox")
            .ok_or_else(|| PdfBackendError::InvalidDocument("MediaBox not found".to_string()))?,
    };

    if let Some(crop_box) = inherited_page_object(document, source_object_id, b"CropBox") {
        dictionary.set("CropBox", crop_box);
    }

    set_page_rotation(&mut dictionary, 0, rotation);

    Ok(Object::Dictionary(dictionary))
}

pub(super) fn normalize_page_sizes(
    document: &mut Document,
    page_ids: &[ObjectId],
) -> Result<(), PdfBackendError> {
    let mut page_boxes = Vec::with_capacity(page_ids.len());
    let mut target_width = 0.0_f32;
    let mut target_height = 0.0_f32;

    for page_id in page_ids {
        let page_box = inherited_page_box(document, *page_id, b"MediaBox")?;
        let (display_width, display_height) =
            page_box.display_size(inherited_page_rotation(document, *page_id));
        target_width = target_width.max(display_width);
        target_height = target_height.max(display_height);
        page_boxes.push((*page_id, page_box));
    }

    for (page_id, page_box) in page_boxes {
        let media_box = page_box.objects_for_display_size(
            target_width,
            target_height,
            inherited_page_rotation(document, page_id),
        );
        let page = document
            .get_object_mut(page_id)
            .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?
            .as_dict_mut()
            .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?;
        page.set("MediaBox", Object::Array(media_box.clone()));
        page.set("CropBox", Object::Array(media_box.clone()));
    }

    Ok(())
}

pub(super) fn inherited_visible_page_box(
    document: &Document,
    object_id: ObjectId,
) -> Result<(f32, f32, f32, f32), PdfBackendError> {
    let page_box = inherited_page_object(document, object_id, b"CropBox")
        .or_else(|| inherited_page_object(document, object_id, b"MediaBox"))
        .ok_or_else(|| PdfBackendError::InvalidDocument("MediaBox not found".to_string()))
        .and_then(parse_page_box)?;

    Ok((
        page_box.left,
        page_box.bottom,
        page_box.width,
        page_box.height,
    ))
}

#[derive(Clone)]
struct PageBox {
    left: f32,
    bottom: f32,
    width: f32,
    height: f32,
}

impl PageBox {
    fn display_size(&self, rotation: i64) -> (f32, f32) {
        if rotates_page_size(rotation) {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    fn objects_for_display_size(
        &self,
        display_width: f32,
        display_height: f32,
        rotation: i64,
    ) -> Vec<Object> {
        let (width, height) = if rotates_page_size(rotation) {
            (display_height, display_width)
        } else {
            (display_width, display_height)
        };

        vec![
            pdf_number(self.left),
            pdf_number(self.bottom),
            pdf_number(self.left + width),
            pdf_number(self.bottom + height),
        ]
    }
}

fn rotates_page_size(rotation: i64) -> bool {
    matches!(normalize_rotation(rotation), 90 | 270)
}

fn inherited_page_box(
    document: &Document,
    object_id: ObjectId,
    key: &[u8],
) -> Result<PageBox, PdfBackendError> {
    let object = inherited_page_object(document, object_id, key).ok_or_else(|| {
        PdfBackendError::InvalidDocument(format!("{} not found", String::from_utf8_lossy(key)))
    })?;
    parse_page_box(object)
}

fn inherited_page_object(
    document: &Document,
    mut object_id: ObjectId,
    key: &[u8],
) -> Option<Object> {
    for _ in 0..document.objects.len() {
        let dictionary = document.get_object(object_id).ok()?.as_dict().ok()?;

        if let Ok(object) = dictionary.get(key) {
            return Some(object.clone());
        }

        object_id = dictionary.get(b"Parent").ok()?.as_reference().ok()?;
    }

    None
}

fn parse_page_box(object: Object) -> Result<PageBox, PdfBackendError> {
    let values = object
        .as_array()
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?;
    if values.len() < 4 {
        return Err(PdfBackendError::InvalidDocument(
            "page box has fewer than four values".to_string(),
        ));
    }

    let left = values[0]
        .as_float()
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?;
    let bottom = values[1]
        .as_float()
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?;
    let right = values[2]
        .as_float()
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?;
    let top = values[3]
        .as_float()
        .map_err(|error| PdfBackendError::InvalidDocument(error.to_string()))?;

    Ok(PageBox {
        left,
        bottom,
        width: right - left,
        height: top - bottom,
    })
}

fn pdf_number(value: f32) -> Object {
    if value.fract().abs() < f32::EPSILON {
        Object::Integer(value as i64)
    } else {
        Object::Real(value)
    }
}
