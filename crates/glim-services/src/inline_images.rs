//! Bounded raster previews for inline document images. Call from a background executor.
use crate::limits as reader_limits;
use anyhow::{Context, Result, ensure};
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
use std::{path::Path, sync::Mutex};

const MAX_EDGE: u32 = reader_limits::INLINE_IMAGE_PREVIEW_EDGE;
// Avoid simultaneous full-size native decode work for a document containing many images.
static PREPARATION: Mutex<()> = Mutex::new(());

pub fn from_file_cancellable(
    path: &Path,
    cancel: &crate::work::Cancellation,
) -> Result<crate::media::RasterFrame> {
    cancel.check()?;
    let _guard = PREPARATION
        .lock()
        .map_err(|_| anyhow::anyhow!("Image preparation was interrupted"))?;
    cancel.check()?;
    native_thumbnail(path, cancel)
}

fn native_thumbnail(
    path: &Path,
    cancel: &crate::work::Cancellation,
) -> Result<crate::media::RasterFrame> {
    let png = thumbnail_png_cancellable(path, MAX_EDGE, cancel)?;
    crate::media::decode_png(&png, cancel)
}

pub(crate) fn thumbnail_png(path: &Path, max_edge: u32) -> Result<Vec<u8>> {
    thumbnail_png_cancellable(path, max_edge, &crate::work::Cancellation::default())
}
#[cfg(target_os = "macos")]
pub(crate) fn thumbnail_png_cancellable(
    path: &Path,
    max_edge: u32,
    cancel: &crate::work::Cancellation,
) -> Result<Vec<u8>> {
    cancel.check()?;
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("preview.png");
    let mut signature = [0; 4];
    use std::io::Read;
    let qoi = std::fs::File::open(path)?.read(&mut signature)? == 4 && &signature == b"qoif";
    let converted = if qoi {
        Some(crate::qoi::prepare(path, cancel)?)
    } else {
        None
    };
    let path = converted.as_ref().map_or(path, |file| file.path());
    let dimensions = image::ImageReader::open(path)
        .ok()
        .and_then(|reader| reader.with_guessed_format().ok())
        .and_then(|reader| reader.into_dimensions().ok());
    let svg_size = if dimensions.is_none() {
        crate::svg_preview::dimensions(path, cancel).ok().flatten()
    } else {
        None
    };
    if let Some(size) = svg_size {
        let svg = crate::svg_preview::prepare_thumbnail(path, size, max_edge, cancel)?;
        let side = size.0.max(size.1);
        let edge = side.ceil().clamp(1., f64::from(max_edge)) as u32;
        let mut command = Command::new("/usr/bin/qlmanage");
        command
            .args(["-t", "-s", &edge.to_string(), "-o"])
            .arg(directory.path())
            .arg(svg.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        ensure!(
            crate::work::run(&mut command, cancel)?.success(),
            "Cannot prepare SVG thumbnail"
        );
        let thumbnail = directory
            .path()
            .join(svg.path().file_name().context("Missing SVG filename")?)
            .with_extension("svg.png");
        let (width, height) = image::ImageReader::open(&thumbnail)?.into_dimensions()?;
        ensure!(
            width > 0 && height > 0 && width <= max_edge && height <= max_edge,
            "Invalid SVG thumbnail dimensions"
        );
        let width = (f64::from(width) * size.0 / side).ceil().max(1.) as u32;
        let height = (f64::from(height) * size.1 / side).ceil().max(1.) as u32;
        let mut reader = image::ImageReader::open(thumbnail)?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(max_edge);
        limits.max_image_height = Some(max_edge);
        limits.max_alloc = Some(reader_limits::INLINE_IMAGE_DECODED_BYTES);
        reader.limits(limits);
        cancel.check()?;
        let image = reader.decode()?;
        cancel.check()?;
        image
            .crop_imm(0, 0, width, height)
            .save_with_format(&output, image::ImageFormat::Png)?;
    } else {
        ensure!(
            sips_thumbnail(path, &output, max_edge, dimensions, cancel)?,
            "Cannot prepare inline image thumbnail"
        );
    }
    ensure!(
        std::fs::metadata(&output)?.len()
            <= u64::from(max_edge) * u64::from(max_edge) * 4
                + reader_limits::INLINE_IMAGE_OUTPUT_OVERHEAD_BYTES,
        "Image thumbnail exceeds output budget"
    );
    let (width, height) = image::ImageReader::open(&output)?.into_dimensions()?;
    ensure!(
        width > 0 && height > 0 && width <= max_edge && height <= max_edge,
        "Invalid thumbnail dimensions"
    );
    Ok(std::fs::read(output)?)
}

#[cfg(target_os = "linux")]
pub(crate) fn thumbnail_png_cancellable(
    path: &Path,
    max_edge: u32,
    cancel: &crate::work::Cancellation,
) -> Result<Vec<u8>> {
    cancel.check()?;
    let reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let qoi = if reader.format() == Some(image::ImageFormat::Qoi) {
        Some(crate::qoi::prepare(path, cancel)?)
    } else {
        None
    };
    let mut reader = if let Some(qoi) = &qoi {
        image::ImageReader::open(qoi.path())?.with_guessed_format()?
    } else {
        reader
    };
    if reader.format().is_none() {
        let size =
            crate::svg_preview::dimensions(path, cancel)?.context("Unsupported image format")?;
        let svg = crate::svg_preview::prepare_thumbnail(path, size, max_edge, cancel)?;
        let tree = resvg::usvg::Tree::from_data(
            &std::fs::read(svg.path())?,
            &resvg::usvg::Options::default(),
        )?;
        let side = size.0.max(size.1);
        let edge = side.ceil().clamp(1., f64::from(max_edge));
        let scale = edge as f32 / tree.size().width();
        let width = (edge * size.0 / side).ceil().max(1.) as u32;
        let height = (edge * size.1 / side).ceil().max(1.) as u32;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
            .context("Invalid SVG thumbnail dimensions")?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        cancel.check()?;
        return Ok(pixmap.encode_png()?);
    }
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(reader_limits::INLINE_IMAGE_DECODED_BYTES);
    reader.limits(limits);
    // Convert float/HDR channels before thumbnail's integer averaging.
    let image = reader.decode()?.to_rgba8();
    let longest = u64::from(image.width().max(image.height()));
    let edge = u64::from(max_edge).min(longest);
    let width = (u64::from(image.width()) * edge / longest).max(1) as u32;
    let height = (u64::from(image.height()) * edge / longest).max(1) as u32;
    let image = image::imageops::thumbnail(&image, width, height);
    cancel.check()?;
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png)?;
    cancel.check()?;
    ensure!(
        png.get_ref().len() as u64
            <= u64::from(max_edge) * u64::from(max_edge) * 4
                + reader_limits::INLINE_IMAGE_OUTPUT_OVERHEAD_BYTES,
        "Image thumbnail exceeds output budget"
    );
    Ok(png.into_inner())
}

#[cfg(target_os = "macos")]
fn sips_thumbnail(
    path: &Path,
    output: &Path,
    max_edge: u32,
    dimensions: Option<(u32, u32)>,
    cancel: &crate::work::Cancellation,
) -> Result<bool> {
    // Specify both dimensions for thin images: sips' longest-edge option can
    // round the short edge to zero even though the source is valid.
    let mut command = Command::new("/usr/bin/sips");
    if let Some((width, height)) = dimensions {
        let longest = u64::from(width.max(height)).max(1);
        let edge = u64::from(max_edge).min(longest);
        let width = (u64::from(width) * edge / longest).max(1);
        let height = (u64::from(height) * edge / longest).max(1);
        command.args([
            "--resampleHeightWidth",
            &height.to_string(),
            &width.to_string(),
        ]);
    } else {
        command.args(["--resampleHeightWidthMax", &max_edge.to_string()]);
    }
    command
        .args(["--setProperty", "format", "png", "--out"])
        .arg(output)
        .arg(std::path::absolute(path)?)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    Ok(crate::work::run(&mut command, cancel)
        .context("Cannot prepare inline image thumbnail")?
        .success())
}
