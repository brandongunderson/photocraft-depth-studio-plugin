use crate::{Result, error};
use serde::{Deserialize, Serialize};
use std::{io::Read, path::PathBuf};

pub const MAX_PIXELS: u64 = 24_000_000;
pub const MAX_SIDE: u32 = 32_768;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Output {
    Blur,
    Map,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BlurSide {
    Background,
    Both,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub output: Output,
    pub focus: f32,
    pub range: f32,
    pub radius: u32,
    pub side: BlurSide,
    pub invert: bool,
}

impl Options {
    pub fn validate(&self) -> Result<()> {
        if !self.focus.is_finite()
            || !(0.0..=1.0).contains(&self.focus)
            || !self.range.is_finite()
            || !(0.0..=1.0).contains(&self.range)
            || self.radius > 64
        {
            return Err(error(
                "Focus and sharp range must be between 0 and 1; blur radius must be 0–64 pixels.",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OutputSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Job {
    pub api_version: u32,
    pub input: PathBuf,
    pub output: PathBuf,
    pub options: Options,
    pub output_size: OutputSize,
    pub color_space: String,
}

pub fn dimensions(width: u32, height: u32) -> Result<usize> {
    let pixels = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE || pixels > MAX_PIXELS {
        return Err(error(
            "Depth Studio accepts images up to 24 megapixels and 32,768 pixels per side. Resize a copy for larger images.",
        ));
    }
    usize::try_from(pixels).map_err(|_| error("Image dimensions exceed this platform's capacity."))
}

pub fn parse_job(reader: impl Read) -> Result<Job> {
    let mut bytes = Vec::new();
    reader.take(16_385).read_to_end(&mut bytes)?;
    if bytes.len() > 16_384 {
        return Err(error("Job request exceeds 16 KiB."));
    }
    let job: Job = serde_json::from_slice(&bytes)?;
    if job.api_version != 1
        || job.color_space != "srgb"
        || !job.input.is_absolute()
        || !job.output.is_absolute()
        || job.input == job.output
    {
        return Err(error(
            "Unsupported worker protocol, color space, or image paths.",
        ));
    }
    dimensions(job.output_size.width, job.output_size.height)?;
    job.options.validate()?;
    if job.output.exists() {
        return Err(error("Output already exists; refusing to overwrite it."));
    }
    Ok(job)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn request(root: &std::path::Path) -> Value {
        json!({"apiVersion":1,"input":root.join("source.png"),"output":root.join("result.png"),
            "outputSize":{"width":12,"height":10},"colorSpace":"srgb",
            "options":{"output":"blur","focus":0.65,"range":0.1,"radius":16,"side":"background","invert":false}})
    }
    fn parse(value: &Value) -> Result<Job> {
        parse_job(serde_json::to_vec(value).unwrap().as_slice())
    }

    #[test]
    fn hostile_jobs_are_rejected_before_loading_model() {
        let temp = tempfile::tempdir().unwrap();
        let good = request(temp.path());
        assert!(parse(&good).is_ok());
        for (field, value) in [
            ("apiVersion", json!(2)),
            ("colorSpace", json!("linear")),
            ("input", json!("relative.png")),
            ("unexpected", json!(true)),
        ] {
            let mut bad = good.clone();
            bad[field] = value;
            assert!(parse(&bad).is_err());
        }
        for (field, value) in [
            ("radius", json!(65)),
            ("radius", json!(-1)),
            ("focus", json!(1.1)),
            ("range", json!(-0.1)),
            ("output", json!("bad")),
            ("side", json!("bad")),
            ("invert", json!(1)),
        ] {
            let mut bad = good.clone();
            bad["options"][field] = value;
            assert!(parse(&bad).is_err());
        }
        let mut bad = good.clone();
        bad["output"] = good["input"].clone();
        assert!(parse(&bad).is_err());
        std::fs::write(temp.path().join("result.png"), b"existing").unwrap();
        assert!(parse(&good).is_err());
        assert!(parse_job(vec![b' '; 16_385].as_slice()).is_err());
    }

    #[test]
    fn dimensions_are_bounded_and_float_options_are_finite() {
        for pair in [
            (0, 1),
            (1, 0),
            (32_769, 1),
            (6000, 4001),
            (u32::MAX, u32::MAX),
        ] {
            assert!(dimensions(pair.0, pair.1).is_err());
        }
        assert_eq!(dimensions(6000, 4000).unwrap(), 24_000_000);
        let mut options = Options {
            output: Output::Blur,
            focus: f32::NAN,
            range: 0.1,
            radius: 16,
            side: BlurSide::Background,
            invert: false,
        };
        assert!(options.validate().is_err());
        options.focus = 0.5;
        options.range = f32::INFINITY;
        assert!(options.validate().is_err());
    }
}
