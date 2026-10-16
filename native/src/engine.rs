use crate::{
    Result, blur, error,
    protocol::{Job, Output, dimensions},
};
use image::{Rgb, RgbImage, RgbaImage, imageops::FilterType};
use ort::{
    session::{RunOptions, Session, builder::GraphOptimizationLevel},
    tensor::TensorElementType,
    value::{Tensor, ValueType},
};
use std::{
    fs::File,
    io::{BufReader, BufWriter},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(error("Cancelled. No result was saved."))
    } else {
        Ok(())
    }
}

/// Model tensors use multiples of the ViT patch size. Bound wide panoramas rather
/// than allowing an arbitrarily long input when the short side is scaled to 518.
pub fn inference_dimensions(width: u32, height: u32) -> Result<(u32, u32)> {
    dimensions(width, height)?;
    let scale = (518.0 / f64::from(width.min(height))).min(1036.0 / f64::from(width.max(height)));
    let aligned = |side: u32| ((f64::from(side) * scale / 14.0).round() as u32).clamp(1, 74) * 14;
    Ok((aligned(width), aligned(height)))
}

pub fn read_source(path: &Path) -> Result<RgbaImage> {
    let mut decoder = png::Decoder::new(BufReader::new(File::open(path)?));
    decoder.set_limits(png::Limits {
        bytes: 256 * 1024 * 1024,
    });
    let mut reader = decoder.read_info()?;
    let info = reader.info();
    let pixels = dimensions(info.width, info.height)?;
    if info.bit_depth != png::BitDepth::Eight
        || info.color_type != png::ColorType::Rgba
        || info.animation_control.is_some()
    {
        return Err(error("Expected one 8-bit RGBA sRGB PNG from PhotoCraft."));
    }
    let (width, height) = (info.width, info.height);
    let bytes = pixels
        .checked_mul(4)
        .ok_or_else(|| error("Image buffer is too large."))?;
    let mut decoded = vec![0; bytes];
    let frame = reader.next_frame(&mut decoded)?;
    if frame.buffer_size() != bytes {
        return Err(error("Invalid source PNG buffer."));
    }
    RgbaImage::from_raw(width, height, decoded).ok_or_else(|| error("Invalid source dimensions."))
}

/// RGB floats in NCHW layout, with ImageNet normalization. Alpha is composited over
/// neutral gray only for estimation; render() always retains the source alpha.
pub fn preprocess(source: &RgbaImage, cancel: &AtomicBool) -> Result<(Vec<f32>, u32, u32)> {
    let (width, height) = inference_dimensions(source.width(), source.height())?;
    let mut rgb = RgbImage::new(source.width(), source.height());
    for (index, (target, pixel)) in rgb.pixels_mut().zip(source.pixels()).enumerate() {
        if index % 65536 == 0 {
            check_cancel(cancel)?;
        }
        let a = u32::from(pixel[3]);
        *target = Rgb(std::array::from_fn(|channel| {
            ((u32::from(pixel[channel]) * a + 128 * (255 - a) + 127) / 255) as u8
        }));
    }
    let resized = image::imageops::resize(&rgb, width, height, FilterType::CatmullRom);
    drop(rgb);
    check_cancel(cancel)?;
    let plane = usize::try_from(u64::from(width) * u64::from(height))?;
    let mut tensor = vec![0.0; plane * 3];
    let means = [0.485, 0.456, 0.406];
    let deviations = [0.229, 0.224, 0.225];
    for (index, pixel) in resized.pixels().enumerate() {
        for channel in 0..3 {
            tensor[channel * plane + index] =
                (f32::from(pixel[channel]) / 255.0 - means[channel]) / deviations[channel];
        }
    }
    Ok((tensor, width, height))
}

fn coordinate(index: usize, from: usize, to: usize) -> (usize, usize, f32) {
    let position = ((index as f32 + 0.5) * from as f32 / to as f32 - 0.5).max(0.0);
    let low = (position.floor() as usize).min(from - 1);
    (low, (low + 1).min(from - 1), position - position.floor())
}

/// Resize in float, normalize using visible pixels, and preserve inverse-depth
/// orientation (larger means nearer). Constant predictions become neutral 0.5.
pub fn normalize_depth(
    source: &RgbaImage,
    values: &[f32],
    width: u32,
    height: u32,
    cancel: &AtomicBool,
) -> Result<Vec<f32>> {
    let count = dimensions(source.width(), source.height())?;
    if width == 0
        || height == 0
        || width > 1036
        || height > 1036
        || values.len() != width as usize * height as usize
        || values.iter().any(|value| !value.is_finite())
    {
        return Err(error("The depth model returned an invalid tensor."));
    }
    let mut resized = vec![0.0; count];
    let sw = source.width() as usize;
    let sh = source.height() as usize;
    let mut minimum = f32::INFINITY;
    let mut maximum = f32::NEG_INFINITY;
    for y in 0..sh {
        check_cancel(cancel)?;
        let (y0, y1, ty) = coordinate(y, height as usize, sh);
        for x in 0..sw {
            let (x0, x1, tx) = coordinate(x, width as usize, sw);
            let at = |px, py| values[py * width as usize + px];
            let value = (at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx) * (1.0 - ty)
                + (at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx) * ty;
            let index = y * sw + x;
            resized[index] = value;
            if source.as_raw()[index * 4 + 3] != 0 {
                minimum = minimum.min(value);
                maximum = maximum.max(value);
            }
        }
    }
    let span = maximum - minimum;
    for (index, value) in resized.iter_mut().enumerate() {
        if index % 65536 == 0 {
            check_cancel(cancel)?;
        }
        *value = if span.is_finite() && span > 1e-6 {
            ((*value - minimum) / span).clamp(0.0, 1.0)
        } else {
            0.5
        };
    }
    Ok(resized)
}

fn infer(
    source: &RgbaImage,
    runtime: &Path,
    cancel: &AtomicBool,
    progress: &dyn Fn(f32, &str),
) -> Result<Vec<f32>> {
    progress(8.0, "Preparing image for depth estimation");
    let (pixels, width, height) = preprocess(source, cancel)?;
    let dll = std::fs::canonicalize(runtime.join("bin/onnxruntime.dll")).map_err(|_| {
        error("Bundled ONNX Runtime is missing. Reinstall the complete Depth Studio plugin.")
    })?;
    let model = std::fs::canonicalize(runtime.join("models/depth-anything-v2-small.onnx"))
        .map_err(|_| {
            error(
                "Bundled Depth Anything V2 Small model is missing. Reinstall the complete plugin.",
            )
        })?;
    progress(15.0, "Loading local depth model");
    std::panic::catch_unwind(||ort::init_from(dll.to_string_lossy()).with_name("PhotoCraft Depth Studio").with_telemetry(false).commit())
        .map_err(|_|error("Cannot load the local AI runtime. Reinstall the complete Windows x64 plugin with its runtime DLLs."))??;
    let threads = std::thread::available_parallelism().map_or(1, |count| count.get().min(4));
    let mut session = Session::builder()?
        .with_log_level(ort::logging::LogLevel::Error)?
        .with_optimization_level(GraphOptimizationLevel::Level1)?
        .with_intra_threads(threads)?
        .with_inter_threads(1)?
        .with_parallel_execution(false)?
        .commit_from_file(model)?;
    let valid_input=session.inputs.len()==1 && session.inputs.first().is_some_and(|input|input.name=="pixel_values" && matches!(&input.input_type,ValueType::Tensor{ty:TensorElementType::Float32,shape,..} if shape.len()==4 && shape.get(1)==Some(&3)));
    if !valid_input
        || session.outputs.len() != 1
        || session
            .outputs
            .first()
            .is_none_or(|output| output.name != "predicted_depth")
    {
        return Err(error(
            "The bundled model input/output signature is incompatible.",
        ));
    }
    check_cancel(cancel)?;
    let tensor = Tensor::from_array((
        [1usize, 3, height as usize, width as usize],
        pixels.into_boxed_slice(),
    ))?;
    progress(25.0, "Estimating relative scene depth locally");
    let options = RunOptions::new()?;
    let finished = AtomicBool::new(false);
    struct Finish<'a>(&'a AtomicBool);
    impl Drop for Finish<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let outputs = std::thread::scope(|scope| {
        scope.spawn(|| {
            while !finished.load(Ordering::Acquire) {
                if cancel.load(Ordering::Acquire) {
                    let _ = options.terminate();
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        });
        let _finish = Finish(&finished);
        session.run_with_options(ort::inputs!["pixel_values"=>tensor], &options)
    });
    check_cancel(cancel)?;
    let outputs = outputs?;
    let output = outputs
        .values()
        .next()
        .ok_or_else(|| error("The depth model returned no result."))?;
    let (shape, values) = output.try_extract_tensor::<f32>()?;
    if shape.as_ref() != [1, i64::from(height), i64::from(width)] {
        return Err(error("The depth model returned unexpected dimensions."));
    }
    let values = values.to_vec();
    drop(outputs);
    drop(session);
    progress(60.0, "Restoring depth to the original image size");
    normalize_depth(source, &values, width, height, cancel)
}

pub fn write_result(image: &RgbaImage, output: &Path, cancel: &AtomicBool) -> Result<()> {
    check_cancel(cancel)?;
    let parent = output
        .parent()
        .ok_or_else(|| error("Missing output directory."))?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    {
        let mut encoder = png::Encoder::new(
            BufWriter::new(temporary.as_file()),
            image.width(),
            image.height(),
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(image.as_raw())?;
        writer.finish()?;
    }
    check_cancel(cancel)?;
    temporary.persist_noclobber(output)?;
    Ok(())
}

pub fn process(
    job: &Job,
    runtime: &Path,
    cancel: &AtomicBool,
    progress: &dyn Fn(f32, &str),
) -> Result<()> {
    job.options.validate()?;
    check_cancel(cancel)?;
    if job.output.exists() {
        return Err(error("Output already exists; refusing to overwrite it."));
    }
    progress(2.0, "Reading source image");
    let source = read_source(&job.input)?;
    if source.dimensions() != (job.output_size.width, job.output_size.height) {
        return Err(error(
            "Depth Studio preserves the source size; output dimensions must match.",
        ));
    }
    let result = if job.options.output == Output::Blur && job.options.radius == 0 {
        source
    } else {
        let depth = infer(&source, runtime, cancel, progress)?;
        blur::render(&source, &depth, &job.options, cancel, progress)?
    };
    progress(97.0, "Saving result");
    write_result(&result, &job.output, cancel)?;
    progress(100.0, "Result ready");
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use image::Rgba;
    #[test]
    fn model_resolution_is_patch_aligned_and_bounded() {
        assert_eq!(inference_dimensions(512, 512).unwrap(), (518, 518));
        assert_eq!(inference_dimensions(6000, 4000).unwrap(), (784, 518));
        for (w, h) in [(32768, 1), (1, 32768), (1, 1), (4000, 6000)] {
            let (a, b) = inference_dimensions(w, h).unwrap();
            assert!(a <= 1036 && b <= 1036 && a % 14 == 0 && b % 14 == 0);
        }
    }
    #[test]
    fn transparent_hidden_color_does_not_affect_model_tensor() {
        let cancel = AtomicBool::new(false);
        let a = RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 0]));
        let b = RgbaImage::from_pixel(2, 2, Rgba([0, 255, 255, 0]));
        assert_eq!(
            preprocess(&a, &cancel).unwrap().0,
            preprocess(&b, &cancel).unwrap().0
        );
        let white = RgbaImage::from_pixel(2, 2, Rgba([255, 255, 255, 255]));
        let (tensor, w, h) = preprocess(&white, &cancel).unwrap();
        let plane = (w * h) as usize;
        assert!((tensor[0] - (1.0 - 0.485) / 0.229).abs() < 1e-5);
        assert!((tensor[plane] - (1.0 - 0.456) / 0.224).abs() < 1e-5);
    }
    #[test]
    fn normalization_is_finite_and_preserves_near_direction() {
        let cancel = AtomicBool::new(false);
        let source = RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]));
        assert_eq!(
            normalize_depth(&source, &[2.0, 4.0, 6.0, 8.0], 2, 2, &cancel).unwrap(),
            vec![0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0]
        );
        assert_eq!(
            normalize_depth(&source, &[4.0; 4], 2, 2, &cancel).unwrap(),
            vec![0.5; 4]
        );
        assert!(normalize_depth(&source, &[f32::NAN; 4], 2, 2, &cancel).is_err());
        assert!(normalize_depth(&source, &[1.0; 3], 2, 2, &cancel).is_err());
    }
    #[test]
    fn png_roundtrip_retains_alpha_and_never_overwrites() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("result.png");
        let cancel = AtomicBool::new(false);
        let source = RgbaImage::from_fn(9, 7, |x, y| Rgba([x as u8, y as u8, 29, (x * y) as u8]));
        write_result(&source, &output, &cancel).unwrap();
        assert_eq!(read_source(&output).unwrap(), source);
        assert!(write_result(&source, &output, &cancel).is_err());
        cancel.store(true, Ordering::Release);
        let second = temp.path().join("cancelled.png");
        assert!(write_result(&source, &second, &cancel).is_err());
        assert!(!second.exists());
    }
}
