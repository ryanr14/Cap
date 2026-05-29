use std::path::PathBuf;

use clap::Args;
use image::RgbaImage;

#[derive(Args)]
#[command(name = "scrolling-capture-stitch")]
pub struct ScrollingCaptureStitch {
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value_t = 720)]
    max_overlap: u32,
    #[arg(long, default_value_t = 24)]
    min_overlap: u32,
    #[arg(long, default_value_t = 6.0)]
    max_diff: f64,
    #[arg(required = true, num_args = 2..)]
    inputs: Vec<PathBuf>,
}

struct LoadedCapture {
    path: PathBuf,
    image: RgbaImage,
}

#[derive(Clone, Copy)]
struct StitchOptions {
    min_overlap: u32,
    max_overlap: u32,
    max_diff: f64,
}

struct StitchSegment {
    path: PathBuf,
    trim_top: u32,
    score: Option<f64>,
}

struct StitchResult {
    image: RgbaImage,
    segments: Vec<StitchSegment>,
}

#[derive(Clone, Copy, Debug)]
struct OverlapMatch {
    rows: u32,
    score: f64,
}

impl ScrollingCaptureStitch {
    pub fn run(self) -> Result<(), String> {
        let options = StitchOptions {
            min_overlap: self.min_overlap,
            max_overlap: self.max_overlap,
            max_diff: self.max_diff,
        };
        validate_options(options)?;

        let captures = self
            .inputs
            .into_iter()
            .map(load_capture)
            .collect::<Result<Vec<_>, _>>()?;
        let result = stitch_captures(&captures, options)?;

        if let Some(parent) = self.output.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create output directory {}: {e}",
                    parent.display()
                )
            })?;
        }

        result
            .image
            .save(&self.output)
            .map_err(|e| format!("Failed to save {}: {e}", self.output.display()))?;

        println!(
            "stitched {} captures into {} ({}x{})",
            result.segments.len(),
            self.output.display(),
            result.image.width(),
            result.image.height()
        );

        for segment in result.segments.iter().skip(1) {
            match segment.score {
                Some(score) => println!(
                    "  {}: trimmed {}px overlap, score {:.2}",
                    segment.path.display(),
                    segment.trim_top,
                    score
                ),
                None => println!("  {}: no overlap trimmed", segment.path.display()),
            }
        }

        Ok(())
    }
}

fn validate_options(options: StitchOptions) -> Result<(), String> {
    if options.min_overlap > options.max_overlap {
        return Err("min-overlap must be less than or equal to max-overlap".to_string());
    }

    if !options.max_diff.is_finite() || options.max_diff < 0.0 {
        return Err("max-diff must be a non-negative finite number".to_string());
    }

    Ok(())
}

fn load_capture(path: PathBuf) -> Result<LoadedCapture, String> {
    let image = image::open(&path)
        .map_err(|e| format!("Failed to open {}: {e}", path.display()))?
        .to_rgba8();

    if image.width() == 0 || image.height() == 0 {
        return Err(format!("Image is empty: {}", path.display()));
    }

    Ok(LoadedCapture { path, image })
}

fn stitch_captures(
    captures: &[LoadedCapture],
    options: StitchOptions,
) -> Result<StitchResult, String> {
    if captures.len() < 2 {
        return Err("At least two captures are required".to_string());
    }

    let width = captures[0].image.width();
    if let Some(capture) = captures
        .iter()
        .find(|capture| capture.image.width() != width)
    {
        return Err(format!(
            "Capture {} has width {}, expected {width}",
            capture.path.display(),
            capture.image.width()
        ));
    }

    let mut segments = vec![StitchSegment {
        path: captures[0].path.clone(),
        trim_top: 0,
        score: None,
    }];

    for pair in captures.windows(2) {
        let overlap = find_overlap(&pair[0].image, &pair[1].image, options);
        segments.push(StitchSegment {
            path: pair[1].path.clone(),
            trim_top: overlap.map(|m| m.rows).unwrap_or(0),
            score: overlap.map(|m| m.score),
        });
    }

    let total_height = captures
        .iter()
        .zip(&segments)
        .try_fold(0u32, |height, (capture, segment)| {
            height.checked_add(capture.image.height().saturating_sub(segment.trim_top))
        })
        .ok_or_else(|| "Stitched image height overflowed u32".to_string())?;

    let mut output = RgbaImage::new(width, total_height);
    let mut y = 0u32;

    for (capture, segment) in captures.iter().zip(&segments) {
        let trim_top = segment.trim_top.min(capture.image.height());
        let visible_height = capture.image.height().saturating_sub(trim_top);
        if visible_height == 0 {
            continue;
        }

        let visible = image::imageops::crop_imm(&capture.image, 0, trim_top, width, visible_height)
            .to_image();
        image::imageops::replace(&mut output, &visible, 0, i64::from(y));
        y = y
            .checked_add(visible_height)
            .ok_or_else(|| "Stitched image height overflowed u32".to_string())?;
    }

    Ok(StitchResult {
        image: output,
        segments,
    })
}

fn find_overlap(
    previous: &RgbaImage,
    next: &RgbaImage,
    options: StitchOptions,
) -> Option<OverlapMatch> {
    let max_rows = options
        .max_overlap
        .min(previous.height())
        .min(next.height());

    if max_rows < options.min_overlap {
        return None;
    }

    let mut best = None;

    for rows in options.min_overlap..=max_rows {
        let candidate = score_overlap(previous, next, rows);
        if best
            .map(|current| overlap_is_better(candidate, current))
            .unwrap_or(true)
        {
            best = Some(candidate);
        }
    }

    best.filter(|m| m.score <= options.max_diff)
}

fn overlap_is_better(candidate: OverlapMatch, current: OverlapMatch) -> bool {
    candidate.score + 0.25 < current.score
        || ((candidate.score - current.score).abs() <= 0.25 && candidate.rows > current.rows)
}

fn score_overlap(previous: &RgbaImage, next: &RgbaImage, rows: u32) -> OverlapMatch {
    let width = previous.width();
    let x_step = (width / 256).max(1) as usize;
    let y_step = (rows / 96).max(1) as usize;
    let mut total_diff = 0u64;
    let mut channel_count = 0u64;

    for y in (0..rows).step_by(y_step) {
        let previous_y = previous.height() - rows + y;

        for x in (0..width).step_by(x_step) {
            let a = previous.get_pixel(x, previous_y).0;
            let b = next.get_pixel(x, y).0;

            total_diff += u64::from(a[0].abs_diff(b[0]));
            total_diff += u64::from(a[1].abs_diff(b[1]));
            total_diff += u64::from(a[2].abs_diff(b[2]));
            channel_count += 3;
        }
    }

    OverlapMatch {
        rows,
        score: total_diff as f64 / channel_count.max(1) as f64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn capture(width: u32, height: u32, start_row: u32) -> RgbaImage {
        let mut image = RgbaImage::new(width, height);

        for y in 0..height {
            let row = start_row + y;
            for x in 0..width {
                image.put_pixel(
                    x,
                    y,
                    Rgba([
                        (row % 251) as u8,
                        ((x * 17) % 251) as u8,
                        ((row * 3 + x) % 251) as u8,
                        255,
                    ]),
                );
            }
        }

        image
    }

    fn loaded(path: &str, image: RgbaImage) -> LoadedCapture {
        LoadedCapture {
            path: PathBuf::from(path),
            image,
        }
    }

    #[test]
    fn detects_exact_manual_scroll_overlap() {
        let previous = capture(12, 10, 0);
        let next = capture(12, 10, 6);
        let options = StitchOptions {
            min_overlap: 2,
            max_overlap: 8,
            max_diff: 0.0,
        };

        let overlap = find_overlap(&previous, &next, options).unwrap();

        assert_eq!(overlap.rows, 4);
        assert_eq!(overlap.score, 0.0);
    }

    #[test]
    fn leaves_non_matching_captures_untrimmed() {
        let captures = vec![
            loaded("a.png", capture(8, 8, 0)),
            loaded("b.png", capture(8, 8, 20)),
        ];
        let result = stitch_captures(
            &captures,
            StitchOptions {
                min_overlap: 2,
                max_overlap: 6,
                max_diff: 0.0,
            },
        )
        .unwrap();

        assert_eq!(result.image.height(), 16);
        assert_eq!(result.segments[1].trim_top, 0);
    }

    #[test]
    fn rejects_mismatched_widths() {
        let captures = vec![
            loaded("a.png", capture(8, 8, 0)),
            loaded("b.png", capture(7, 8, 4)),
        ];
        let err = match stitch_captures(
            &captures,
            StitchOptions {
                min_overlap: 2,
                max_overlap: 6,
                max_diff: 0.0,
            },
        ) {
            Ok(_) => panic!("expected width mismatch"),
            Err(err) => err,
        };

        assert!(err.contains("width"));
    }
}
