use gtk::gdk_pixbuf::{Colorspace, Pixbuf};
use gtk::prelude::*;
use gtk::{gdk, glib};
use std::path::Path;

#[derive(Debug, Clone)]
pub(crate) struct Argb32Image {
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) stride: usize,
    pub(crate) pixels: Vec<u8>,
}

impl Argb32Image {
    pub(crate) fn new(width: i32, height: i32, stride: usize, pixels: Vec<u8>) -> Option<Self> {
        let pixels = argb32_pixels(width, height, stride, &pixels)?.to_vec();
        Some(Self {
            width,
            height,
            stride,
            pixels,
        })
    }

    pub(crate) fn from_surface(surface: &mut cairo::ImageSurface) -> Result<Self, String> {
        let (width, height, stride, pixels) = argb32_surface_data(surface)?;
        Self::new(width, height, stride, pixels)
            .ok_or_else(|| "unsupported image format".to_string())
    }

    pub(crate) fn surface(&self) -> Option<cairo::ImageSurface> {
        argb32_surface(self.width, self.height, self.stride, &self.pixels)
    }

    pub(crate) fn texture(&self) -> Option<gdk::Texture> {
        argb32_texture(self.width, self.height, self.stride, &self.pixels)
    }

    pub(crate) fn rotated(&self, rotation: i64) -> Option<Self> {
        rotated_argb32_image(self.width, self.height, self.stride, &self.pixels, rotation)
    }

    pub(crate) fn rotated_size(&self, rotation: i64) -> (i32, i32) {
        rotated_size(self.width, self.height, rotation)
    }
}

pub(crate) fn load_pixbuf(path: &Path) -> Result<Pixbuf, String> {
    Pixbuf::from_file(path)
        .map_err(|error| error.to_string())
        .or_else(|error| load_png_pixbuf(path).map_err(|_| error))
}

fn load_png_pixbuf(path: &Path) -> Result<Pixbuf, String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let surface =
        cairo::ImageSurface::create_from_png(&mut file).map_err(|error| error.to_string())?;
    pixbuf_from_surface(surface)
}

fn pixbuf_from_surface(source: cairo::ImageSurface) -> Result<Pixbuf, String> {
    let width = source.width();
    let height = source.height();
    if width <= 0 || height <= 0 {
        return Err("unsupported image format".to_string());
    }

    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)
        .map_err(|error| error.to_string())?;
    let context = cairo::Context::new(&surface).map_err(|error| error.to_string())?;
    context.set_operator(cairo::Operator::Source);
    context
        .set_source_surface(&source, 0.0, 0.0)
        .map_err(|error| error.to_string())?;
    context.paint().map_err(|error| error.to_string())?;
    drop(context);

    let (_, _, stride, data) = argb32_surface_data(&mut surface)?;
    let mut pixels = vec![0; width as usize * height as usize * 4];

    for y in 0..height as usize {
        let source_row = y * stride;
        let target_row = y * width as usize * 4;
        for x in 0..width as usize {
            let source = source_row + x * 4;
            let target = target_row + x * 4;
            let (red, green, blue, alpha) = argb32_pixel(&data[source..source + 4]);
            pixels[target] = unpremultiply(red, alpha);
            pixels[target + 1] = unpremultiply(green, alpha);
            pixels[target + 2] = unpremultiply(blue, alpha);
            pixels[target + 3] = alpha;
        }
    }

    Ok(Pixbuf::from_mut_slice(
        pixels,
        Colorspace::Rgb,
        true,
        8,
        width,
        height,
        width * 4,
    ))
}

pub(crate) fn argb32_surface_data(
    surface: &mut cairo::ImageSurface,
) -> Result<(i32, i32, usize, Vec<u8>), String> {
    surface.flush();

    Ok((
        surface.width(),
        surface.height(),
        surface.stride() as usize,
        surface.data().map_err(|error| error.to_string())?.to_vec(),
    ))
}

pub(crate) fn argb32_surface(
    width: i32,
    height: i32,
    stride: usize,
    pixels: &[u8],
) -> Option<cairo::ImageSurface> {
    let pixels = argb32_pixels(width, height, stride, pixels)?;

    cairo::ImageSurface::create_for_data(
        pixels.to_vec(),
        cairo::Format::ARgb32,
        width,
        height,
        i32::try_from(stride).ok()?,
    )
    .ok()
}

pub(crate) fn argb32_texture(
    width: i32,
    height: i32,
    stride: usize,
    pixels: &[u8],
) -> Option<gdk::Texture> {
    let pixels = argb32_pixels(width, height, stride, pixels)?;

    Some(
        gdk::MemoryTexture::new(
            width,
            height,
            argb32_memory_format(),
            &glib::Bytes::from(pixels),
            stride,
        )
        .upcast(),
    )
}

pub(crate) fn argb32_surface_texture(surface: &mut cairo::ImageSurface) -> Option<gdk::Texture> {
    let (width, height, stride, pixels) = argb32_surface_data(surface).ok()?;
    argb32_texture(width, height, stride, &pixels)
}

fn argb32_required_len(width: i32, height: i32, stride: usize) -> Option<usize> {
    if width <= 0 || height <= 0 || stride < width as usize * 4 {
        return None;
    }

    stride.checked_mul(height as usize)
}

fn argb32_pixels(width: i32, height: i32, stride: usize, pixels: &[u8]) -> Option<&[u8]> {
    let required_len = argb32_required_len(width, height, stride)?;
    pixels.get(..required_len)
}

pub(crate) fn rotated_argb32_image(
    width: i32,
    height: i32,
    stride: usize,
    pixels: &[u8],
    rotation: i64,
) -> Option<Argb32Image> {
    let pixels = argb32_pixels(width, height, stride, pixels)?;
    match normalize_rotation(rotation) {
        90 | 180 | 270 => rotate_argb32_image(width, height, stride, pixels, rotation),
        _ => Argb32Image::new(width, height, stride, pixels.to_vec()),
    }
}

fn rotate_argb32_image(
    source_width: i32,
    source_height: i32,
    source_stride: usize,
    source_pixels: &[u8],
    rotation: i64,
) -> Option<Argb32Image> {
    let (width, height) = rotated_size(source_width, source_height, rotation);
    let stride = width as usize * 4;
    let mut pixels = vec![0; stride * height as usize];

    for source_y in 0..source_height as usize {
        for source_x in 0..source_width as usize {
            let (target_x, target_y) = rotated_pixel_position(
                source_x,
                source_y,
                source_width as usize,
                source_height as usize,
                rotation,
            );
            let source = source_y * source_stride + source_x * 4;
            let target = target_y * stride + target_x * 4;
            pixels[target..target + 4].copy_from_slice(&source_pixels[source..source + 4]);
        }
    }

    Argb32Image::new(width, height, stride, pixels)
}

pub(crate) fn rotated_size(width: i32, height: i32, rotation: i64) -> (i32, i32) {
    if matches!(normalize_rotation(rotation), 90 | 270) {
        (height, width)
    } else {
        (width, height)
    }
}

fn rotated_pixel_position(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    rotation: i64,
) -> (usize, usize) {
    match normalize_rotation(rotation) {
        90 => (height - 1 - y, x),
        180 => (width - 1 - x, height - 1 - y),
        270 => (y, width - 1 - x),
        _ => (x, y),
    }
}

fn normalize_rotation(rotation: i64) -> i64 {
    rotation.rem_euclid(360)
}

fn argb32_memory_format() -> gdk::MemoryFormat {
    gdk::MemoryFormat::B8g8r8a8Premultiplied
}

fn argb32_pixel(pixel: &[u8]) -> (u8, u8, u8, u8) {
    (pixel[2], pixel[1], pixel[0], pixel[3])
}

fn unpremultiply(value: u8, alpha: u8) -> u8 {
    if alpha == 0 {
        0
    } else {
        ((value as u16 * u8::MAX as u16) / alpha as u16).min(u8::MAX as u16) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::{
        argb32_surface, argb32_texture, load_pixbuf, pixbuf_from_surface, rotated_argb32_image,
        rotated_size, Argb32Image,
    };
    use gtk::gdk::prelude::*;

    #[test]
    fn argb32_data_validation_rejects_invalid_buffers() {
        assert!(Argb32Image::new(0, 1, 4, vec![0; 4]).is_none());
        assert!(Argb32Image::new(1, 1, 3, vec![0; 4]).is_none());
        assert!(Argb32Image::new(1, 1, 4, vec![0; 3]).is_none());
        assert!(Argb32Image::new(1, 1, 4, vec![0; 4]).is_some());
    }

    #[test]
    fn argb32_texture_rejects_short_buffers() {
        assert!(argb32_texture(1, 1, 4, &[0; 3]).is_none());
    }

    #[test]
    fn argb32_texture_preserves_native_argb32_channels() {
        let pixel = opaque_red_argb32_pixel();
        let texture = argb32_texture(1, 1, 4, &pixel).expect("valid data should create texture");
        let mut data = vec![0; 4];

        texture.download(&mut data, 4);

        assert_eq!(data, pixel);
    }

    #[test]
    fn argb32_surface_uses_valid_buffer_data() {
        let surface =
            argb32_surface(1, 1, 4, &[0, 0, 0, 255]).expect("valid data should create surface");

        assert_eq!(surface.width(), 1);
        assert_eq!(surface.height(), 1);
    }

    #[test]
    fn pixbuf_from_surface_unpremultiplies_alpha() {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1)
            .expect("surface should be created");
        let context = cairo::Context::new(&surface).expect("context should be created");
        context.set_source_rgba(1.0, 0.0, 0.0, 0.5);
        context.paint().expect("surface should be painted");
        drop(context);

        let pixbuf = pixbuf_from_surface(surface).expect("surface should convert");
        let bytes = pixbuf.read_pixel_bytes();
        let pixels = bytes.as_ref();

        assert_eq!(pixbuf.width(), 1);
        assert_eq!(pixbuf.height(), 1);
        assert!(pixbuf.has_alpha());
        assert_eq!(pixels[0], 255);
        assert_eq!(pixels[1], 0);
        assert_eq!(pixels[2], 0);
        assert!(pixels[3] > 0);
    }

    #[test]
    fn load_pixbuf_reads_png_images() {
        let dir = tempfile::tempdir().expect("test directory should be created");
        let path = dir.path().join("image.png");
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 2, 1)
            .expect("surface should be created");
        let context = cairo::Context::new(&surface).expect("context should be created");
        context.set_source_rgb(0.0, 0.0, 1.0);
        context.paint().expect("surface should be painted");
        drop(context);

        let mut file = std::fs::File::create(&path).expect("png file should be created");
        surface
            .write_to_png(&mut file)
            .expect("png file should be written");

        let pixbuf = load_pixbuf(&path).expect("png image should load");

        assert_eq!(pixbuf.width(), 2);
        assert_eq!(pixbuf.height(), 1);
    }

    #[test]
    fn rotated_argb32_image_moves_pixels_for_quarter_turns() {
        let pixels = label_pixels(&[1, 2, 3, 4, 5, 6]);

        let clockwise = rotated_argb32_image(2, 3, 8, &pixels, 90)
            .expect("valid image should rotate clockwise");
        let upside_down = rotated_argb32_image(2, 3, 8, &pixels, 180)
            .expect("valid image should rotate upside down");
        let counterclockwise = rotated_argb32_image(2, 3, 8, &pixels, 270)
            .expect("valid image should rotate counterclockwise");

        assert_eq!(
            (clockwise.width, clockwise.height, clockwise.stride),
            (3, 2, 12)
        );
        assert_eq!(pixel_labels(&clockwise), vec![5, 3, 1, 6, 4, 2]);
        assert_eq!(
            (upside_down.width, upside_down.height, upside_down.stride),
            (2, 3, 8)
        );
        assert_eq!(pixel_labels(&upside_down), vec![6, 5, 4, 3, 2, 1]);
        assert_eq!(
            (
                counterclockwise.width,
                counterclockwise.height,
                counterclockwise.stride
            ),
            (3, 2, 12)
        );
        assert_eq!(pixel_labels(&counterclockwise), vec![2, 4, 6, 1, 3, 5]);
    }

    #[test]
    fn rotated_size_normalizes_negative_rotation() {
        assert_eq!(rotated_size(100, 200, -90), (200, 100));
        assert_eq!(rotated_size(100, 200, -180), (100, 200));
    }

    fn label_pixels(labels: &[u8]) -> Vec<u8> {
        labels
            .iter()
            .flat_map(|label| [*label, 0, 0, u8::MAX])
            .collect()
    }

    fn pixel_labels(image: &Argb32Image) -> Vec<u8> {
        image.pixels.chunks_exact(4).map(|pixel| pixel[0]).collect()
    }

    fn opaque_red_argb32_pixel() -> [u8; 4] {
        [0, 0, u8::MAX, u8::MAX]
    }
}
