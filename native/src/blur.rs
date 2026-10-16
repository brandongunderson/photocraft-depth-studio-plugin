//! Depth visualization and bounded-memory, depth-guided photographic defocus.
//!
//! The three separable box passes approximate a smooth blur at increasing scales.
//! Far and near regions are filtered separately, excluding the sharp focus band,
//! so a sharp foreground does not stain the blurred background. Filtering uses
//! alpha-weighted linear-light colors and normalizes by filtered coverage. This
//! is a practical depth effect, not a physical lens or an occlusion reconstruction.

use std::sync::atomic::{AtomicBool, Ordering};

use image::RgbaImage;

use crate::protocol::{BlurSide, Options, Output};

type Sample = [f32; 4];

// Keep vertical-pass scratch independent of image height and below about 8 MiB.
const SCRATCH_SAMPLES: usize = 8 * 1024 * 1024 / std::mem::size_of::<Sample>();
const CANCEL_INTERVAL: usize = 8192;

pub fn render(
    source: &RgbaImage,
    depth: &[f32],
    options: &Options,
    cancel: &AtomicBool,
    progress: &dyn Fn(f32, &str),
) -> crate::Result<RgbaImage> {
    check_cancel(cancel)?;
    let report = |fraction: f32, message: &str| progress(65.0 + fraction * 30.0, message);
    let width = source.width() as usize;
    let height = source.height() as usize;
    let count = width
        .checked_mul(height)
        .ok_or_else(|| crate::error("Image dimensions are too large"))?;
    if width == 0 || height == 0 || depth.len() != count {
        return Err(crate::error(
            "Depth dimensions do not match the source image",
        ));
    }
    for (index, value) in depth.iter().enumerate() {
        if index % CANCEL_INTERVAL == 0 {
            check_cancel(cancel)?;
        }
        if !value.is_finite() || !(0.0..=1.0).contains(value) {
            return Err(crate::error(
                "Depth values must be finite and between zero and one",
            ));
        }
    }

    let mut output = source.clone();
    if matches!(options.output, Output::Map) {
        report(0.0, "Rendering depth map");
        for (index, pixel) in output.pixels_mut().enumerate() {
            if index % CANCEL_INTERVAL == 0 {
                check_cancel(cancel)?;
            }
            let value = if options.invert {
                1.0 - depth[index]
            } else {
                depth[index]
            };
            let gray = (value * 255.0).round() as u8;
            pixel.0[..3].fill(gray);
        }
        check_cancel(cancel)?;
        report(1.0, "Depth map ready");
        return Ok(output);
    }

    if !options.focus.is_finite()
        || !(0.0..=1.0).contains(&options.focus)
        || !options.range.is_finite()
        || !(0.0..=1.0).contains(&options.range)
        || options.radius > 64
    {
        return Err(crate::error(
            "Depth blur settings are outside their supported range",
        ));
    }
    if options.radius == 0 {
        check_cancel(cancel)?;
        report(1.0, "Zero-radius blur preserves the source");
        return Ok(output);
    }

    let lower = (options.focus - options.range * 0.5).max(0.0);
    let upper = (options.focus + options.range * 0.5).min(1.0);
    let include_near = matches!(options.side, BlurSide::Both);
    let has_far = depth.iter().any(|&value| value < lower);
    let has_near = include_near && depth.iter().any(|&value| value > upper);
    let regions = usize::from(has_far) + usize::from(has_near);
    if regions == 0 {
        report(1.0, "All pixels are in focus");
        return Ok(output);
    }

    let decode: [f32; 256] = std::array::from_fn(|index| {
        let value = index as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    });
    // A 16-bit linear-light lookup avoids millions of pow calls. Its rounding
    // resolution is finer than one output byte, including near black.
    let encode: Vec<u8> = (0..=u16::MAX)
        .map(|index| {
            let value = f32::from(index) / f32::from(u16::MAX);
            let srgb = if value <= 0.003_130_8 {
                value * 12.92
            } else {
                1.055 * value.powf(1.0 / 2.4) - 0.055
            };
            (srgb * 255.0).round() as u8
        })
        .collect();
    check_cancel(cancel)?;
    let radius = options.radius as usize;
    // Their sum is the requested maximum spatial support. Increasing scales
    // let intermediate depths interpolate between blur levels, not just fade
    // a single heavily blurred image over an otherwise sharp source.
    let first = radius / 6;
    let second = radius / 3;
    let radii: Vec<_> = [first, second, radius - first - second]
        .into_iter()
        .filter(|&value| value != 0)
        .collect();
    let mut work = allocate_samples(count)?;
    let mut completed_regions = 0;

    for far in [true, false] {
        if (far && !has_far) || (!far && !has_near) {
            continue;
        }
        let boundary = if far { lower } else { upper };
        report(
            completed_regions as f32 / regions as f32,
            if far {
                "Blurring background"
            } else {
                "Blurring foreground"
            },
        );
        for (index, (pixel, sample)) in source.pixels().zip(work.iter_mut()).enumerate() {
            if index % CANCEL_INTERVAL == 0 {
                check_cancel(cancel)?;
            }
            let belongs = if far {
                depth[index] < boundary
            } else {
                depth[index] > boundary
            };
            let alpha = if belongs {
                f32::from(pixel[3]) / 255.0
            } else {
                0.0
            };
            *sample = [
                decode[pixel[0] as usize] * alpha,
                decode[pixel[1] as usize] * alpha,
                decode[pixel[2] as usize] * alpha,
                alpha,
            ];
        }

        for (stage, &pass_radius) in radii.iter().enumerate() {
            horizontal_box(&mut work, width, height, pass_radius, cancel)?;
            vertical_box(&mut work, width, height, pass_radius, cancel)?;
            for (index, pixel) in output.pixels_mut().enumerate() {
                if index % CANCEL_INTERVAL == 0 {
                    check_cancel(cancel)?;
                }
                if pixel[3] == 0 || work[index][3] <= f32::EPSILON {
                    continue;
                }
                let strength = blur_strength(depth[index], boundary, far);
                let blend = (strength * radii.len() as f32 - stage as f32).clamp(0.0, 1.0);
                if blend == 0.0 {
                    continue;
                }
                for channel in 0..3 {
                    let current = decode[pixel[channel] as usize];
                    let blurred = work[index][channel] / work[index][3];
                    let linear = current + (blurred - current) * blend;
                    let encoded_index =
                        (linear.clamp(0.0, 1.0) * f32::from(u16::MAX)).round() as usize;
                    pixel[channel] = encode[encoded_index];
                }
            }
            report(
                (completed_regions as f32 + (stage + 1) as f32 / radii.len() as f32)
                    / regions as f32,
                if far {
                    "Blurring background"
                } else {
                    "Blurring foreground"
                },
            );
        }
        completed_regions += 1;
    }
    check_cancel(cancel)?;
    report(1.0, "Depth blur ready");
    Ok(output)
}

fn blur_strength(depth: f32, boundary: f32, far: bool) -> f32 {
    let distance = if far {
        boundary - depth
    } else {
        depth - boundary
    };
    let extent = if far { boundary } else { 1.0 - boundary };
    if distance <= 0.0 || extent <= 0.0 {
        return 0.0;
    }
    let amount = (distance / extent).clamp(0.0, 1.0);
    amount * amount * (3.0 - 2.0 * amount)
}

fn check_cancel(cancel: &AtomicBool) -> crate::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(crate::error("Processing cancelled"))
    } else {
        Ok(())
    }
}

fn allocate_samples(count: usize) -> crate::Result<Vec<Sample>> {
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(count)
        .map_err(|_| crate::error("Not enough memory for depth blur"))?;
    samples.resize(count, [0.0; 4]);
    Ok(samples)
}

fn horizontal_box(
    data: &mut [Sample],
    width: usize,
    height: usize,
    radius: usize,
    cancel: &AtomicBool,
) -> crate::Result<()> {
    let mut row = allocate_samples(width)?;
    let divisor = (radius * 2 + 1) as f32;
    for y in 0..height {
        check_cancel(cancel)?;
        let target = &mut data[y * width..(y + 1) * width];
        row.copy_from_slice(target);
        let mut sum = [0.0; 4];
        for offset in 0..=radius * 2 {
            let index = offset.saturating_sub(radius).min(width - 1);
            for (total, &value) in sum.iter_mut().zip(&row[index]) {
                *total += value;
            }
        }
        for (x, sample) in target.iter_mut().enumerate() {
            if x % CANCEL_INTERVAL == 0 {
                check_cancel(cancel)?;
            }
            for (value, &total) in sample.iter_mut().zip(&sum) {
                *value = total / divisor;
            }
            let leaving = x.saturating_sub(radius);
            let entering = (x + radius + 1).min(width - 1);
            for (channel, total) in sum.iter_mut().enumerate() {
                *total += row[entering][channel] - row[leaving][channel];
            }
        }
    }
    Ok(())
}

fn vertical_box(
    data: &mut [Sample],
    width: usize,
    height: usize,
    radius: usize,
    cancel: &AtomicBool,
) -> crate::Result<()> {
    let ring_rows = (radius + 1).min(height);
    let band_width = (SCRATCH_SAMPLES / ring_rows).max(1).min(width);
    let divisor = (radius * 2 + 1) as f32;
    let mut ring = allocate_samples(ring_rows * band_width)?;
    let mut sums = allocate_samples(band_width)?;
    for start_x in (0..width).step_by(band_width) {
        check_cancel(cancel)?;
        let count = band_width.min(width - start_x);
        sums[..count].fill([0.0; 4]);
        for offset in 0..=radius * 2 {
            let y = offset.saturating_sub(radius).min(height - 1);
            for (sum, sample) in sums[..count]
                .iter_mut()
                .zip(&data[y * width + start_x..y * width + start_x + count])
            {
                for (total, &value) in sum.iter_mut().zip(sample) {
                    *total += value;
                }
            }
        }
        for y in 0..height {
            check_cancel(cancel)?;
            let slot = (y % ring_rows) * band_width;
            let row_start = y * width + start_x;
            ring[slot..slot + count].copy_from_slice(&data[row_start..row_start + count]);
            for (sample, sum) in data[row_start..row_start + count].iter_mut().zip(&sums) {
                for (value, &total) in sample.iter_mut().zip(sum) {
                    *value = total / divisor;
                }
            }
            if y + 1 == height {
                break;
            }
            let leaving_slot = (y.saturating_sub(radius) % ring_rows) * band_width;
            let entering_start = (y + radius + 1).min(height - 1) * width + start_x;
            for x in 0..count {
                for channel in 0..4 {
                    sums[x][channel] +=
                        data[entering_start + x][channel] - ring[leaving_slot + x][channel];
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use image::Rgba;

    fn options() -> Options {
        Options {
            output: Output::Blur,
            focus: 0.7,
            range: 0.1,
            radius: 12,
            side: BlurSide::Background,
            invert: false,
        }
    }

    fn run(image: &RgbaImage, depth: &[f32], options: &Options) -> RgbaImage {
        render(image, depth, options, &AtomicBool::new(false), &|_, _| {}).unwrap()
    }

    #[test]
    fn map_is_white_near_invertible_and_preserves_alpha() {
        let image = RgbaImage::from_fn(3, 1, |x, _| Rgba([15, 25, 35, (x * 100) as u8]));
        let mut settings = options();
        settings.output = Output::Map;
        let map = run(&image, &[0.0, 0.5, 1.0], &settings);
        assert_eq!(map.get_pixel(0, 0).0, [0, 0, 0, 0]);
        assert_eq!(map.get_pixel(1, 0).0, [128, 128, 128, 100]);
        assert_eq!(map.get_pixel(2, 0).0, [255, 255, 255, 200]);
        settings.invert = true;
        let inverted = run(&image, &[0.0, 0.5, 1.0], &settings);
        assert_eq!(inverted.get_pixel(0, 0).0, [255, 255, 255, 0]);
        assert_eq!(inverted.get_pixel(2, 0).0, [0, 0, 0, 200]);
        let constant = run(&image, &[0.25; 3], &settings);
        assert!(constant.pixels().all(|pixel| pixel[0] == 191));
    }

    #[test]
    fn radius_zero_preserves_every_byte() {
        let image = RgbaImage::from_fn(17, 13, |x, y| {
            Rgba([
                (x * 11) as u8,
                (y * 17) as u8,
                ((x + y) * 7) as u8,
                (x * y) as u8,
            ])
        });
        let mut settings = options();
        settings.radius = 0;
        assert_eq!(run(&image, &vec![0.0; 17 * 13], &settings), image);
    }

    #[test]
    fn constant_visible_color_survives_with_nonuniform_alpha() {
        let image = RgbaImage::from_fn(31, 23, |x, y| {
            Rgba([43, 127, 219, ((x * 13 + y * 19) % 256) as u8])
        });
        let result = run(&image, &vec![0.0; 31 * 23], &options());
        assert_eq!(result, image);
    }

    #[test]
    fn transparent_hidden_colors_do_not_make_fringes() {
        let image = RgbaImage::from_fn(41, 9, |x, _| {
            if x < 20 {
                Rgba([0, 0, 255, 0])
            } else {
                Rgba([255, 0, 0, 127])
            }
        });
        let result = run(&image, &vec![0.0; 41 * 9], &options());
        assert_eq!(result, image);
    }

    #[test]
    fn filtered_colors_preserve_original_alpha_and_use_linear_light() {
        let image = RgbaImage::from_fn(23, 17, |x, y| {
            Rgba([
                (x * 11) as u8,
                (y * 15) as u8,
                ((x + y) * 6) as u8,
                ((x * y) % 256) as u8,
            ])
        });
        let result = run(&image, &vec![0.0; 23 * 17], &options());
        assert_ne!(result, image);
        assert!(
            result
                .pixels()
                .zip(image.pixels())
                .all(|(after, before)| after[3] == before[3])
        );

        let image = RgbaImage::from_fn(3, 1, |x, _| {
            if x == 1 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255; 4])
            }
        });
        let mut settings = options();
        settings.radius = 1;
        let result = run(&image, &[0.0; 3], &settings);
        // Two white neighbors and one black center average to ~213 in sRGB,
        // rather than the ~170 produced by averaging encoded bytes directly.
        assert!((212..=214).contains(&result.get_pixel(1, 0)[0]));
    }

    #[test]
    fn small_radii_still_affect_intermediate_depths() {
        let image = RgbaImage::from_fn(9, 1, |x, _| {
            if x % 2 == 0 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255; 4])
            }
        });
        for radius in [1, 2] {
            let mut settings = options();
            settings.radius = radius;
            assert_ne!(run(&image, &[0.3; 9], &settings), image);
        }
    }

    #[test]
    fn near_foreground_does_not_contaminate_far_background() {
        let image = RgbaImage::from_fn(41, 17, |x, _| {
            if (18..23).contains(&x) {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        });
        let depth: Vec<_> = (0..17)
            .flat_map(|_| (0..41).map(|x| if (18..23).contains(&x) { 0.9 } else { 0.0 }))
            .collect();
        assert_eq!(run(&image, &depth, &options()), image);
        let mut settings = options();
        settings.side = BlurSide::Both;
        assert_eq!(run(&image, &depth, &settings), image);
    }

    #[test]
    fn focused_pixels_stay_exact_and_side_controls_foreground() {
        let image = RgbaImage::from_fn(39, 15, |x, y| {
            if (x + y) % 2 == 0 {
                Rgba([0, 0, 0, 255])
            } else {
                Rgba([255, 255, 255, 255])
            }
        });
        let depth: Vec<_> = (0..15)
            .flat_map(|_| {
                (0..39).map(|x| {
                    if x < 13 {
                        0.0
                    } else if x < 26 {
                        0.7
                    } else {
                        1.0
                    }
                })
            })
            .collect();
        let background = run(&image, &depth, &options());
        assert_ne!(background.get_pixel(5, 5), image.get_pixel(5, 5));
        for y in 0..15 {
            for x in 13..39 {
                assert_eq!(background.get_pixel(x, y), image.get_pixel(x, y));
            }
        }
        let mut settings = options();
        settings.side = BlurSide::Both;
        let both = run(&image, &depth, &settings);
        assert_ne!(both.get_pixel(33, 5), image.get_pixel(33, 5));
        for y in 0..15 {
            for x in 13..26 {
                assert_eq!(both.get_pixel(x, y), image.get_pixel(x, y));
            }
        }
    }

    #[test]
    fn single_pixel_and_thin_images_are_stable_at_maximum_radius() {
        let mut settings = options();
        settings.radius = 64;
        for (width, height) in [(1, 1), (1, 53), (53, 1), (2, 3)] {
            let image = RgbaImage::from_pixel(width, height, Rgba([93, 51, 177, 213]));
            assert_eq!(
                run(&image, &vec![0.0; (width * height) as usize], &settings),
                image
            );
        }
    }

    #[test]
    fn box_passes_match_a_small_reference_including_edges() {
        let width = 7;
        let height = 4;
        let source: Vec<Sample> = (0..width * height)
            .map(|index| [index as f32, (index % 3) as f32, 0.5, 1.0])
            .collect();
        let cancel = AtomicBool::new(false);
        for radius in [1, 3, 8] {
            let mut actual = source.clone();
            horizontal_box(&mut actual, width, height, radius, &cancel).unwrap();
            vertical_box(&mut actual, width, height, radius, &cancel).unwrap();
            let diameter = radius * 2 + 1;
            for y in 0..height {
                for x in 0..width {
                    let mut expected = [0.0; 4];
                    for oy in 0..diameter {
                        for ox in 0..diameter {
                            let sx = (x + ox).saturating_sub(radius).min(width - 1);
                            let sy = (y + oy).saturating_sub(radius).min(height - 1);
                            for (channel, value) in expected.iter_mut().enumerate() {
                                *value += source[sy * width + sx][channel];
                            }
                        }
                    }
                    for (channel, expected) in expected.into_iter().enumerate() {
                        assert!(
                            (actual[y * width + x][channel]
                                - expected / (diameter * diameter) as f32)
                                .abs()
                                < 0.0001
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn rejects_invalid_depth_and_observes_cancellation() {
        let image = RgbaImage::from_pixel(2, 1, Rgba([10, 20, 30, 255]));
        let cancel = AtomicBool::new(false);
        for depth in [
            vec![0.0],
            vec![f32::NAN, 0.0],
            vec![f32::INFINITY, 0.0],
            vec![-0.1, 0.0],
            vec![1.1, 0.0],
        ] {
            assert!(render(&image, &depth, &options(), &cancel, &|_, _| {}).is_err());
        }
        cancel.store(true, Ordering::Relaxed);
        assert!(render(&image, &[0.0; 2], &options(), &cancel, &|_, _| {}).is_err());
        cancel.store(false, Ordering::Relaxed);
        assert!(
            render(&image, &[0.0; 2], &options(), &cancel, &|_, _| {
                cancel.store(true, Ordering::Relaxed);
            })
            .is_err()
        );
    }
}
