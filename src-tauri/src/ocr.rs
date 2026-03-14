use std::{
    collections::HashMap,
    env,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::{
    imageops::FilterType, DynamicImage, GenericImageView, GrayImage, ImageBuffer, ImageFormat,
    Luma, Rgba,
};

use crate::mrtd_parser;

const MRZ_ALLOWED_CHARS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789<";
const MRZ_LEFT_MARGIN_PX: u32 = 12;
const MRZ_PARALLEL_ATTEMPTS: usize = 3;

pub fn run_ocr_from_base64(image_base64: &str) -> Result<String, String> {
    let (bytes, format) = decode_image_payload(image_base64)?;
    let temp_path = write_temp_image(&bytes, format)?;
    let ocr_result = run_tesseract(&temp_path);
    let _ = fs::remove_file(&temp_path);
    ocr_result
}

pub fn run_mrz_ocr_from_base64(image_base64: &str) -> Result<String, String> {
    let (bytes, format) = decode_image_payload(image_base64)?;
    let image = image::load_from_memory_with_format(&bytes, format)
        .map_err(|err| format!("invalid image: {err}"))?;
    run_mrz_ocr_on_image(&image)
}

pub fn run_mrz_ocr_on_path(path: &Path) -> Result<String, String> {
    let image = image::open(path).map_err(|err| format!("cannot open image {}: {err}", path.display()))?;
    run_mrz_ocr_on_image(&image)
}

pub fn extract_mrz_candidate_lines(raw_ocr: &str) -> Vec<String> {
    raw_ocr
        .lines()
        .map(normalize_mrz_line_for_scoring)
        .filter(|line| line.len() >= 30)
        .map(|line| normalize_td3_line_length(&line))
        .collect()
}

pub fn extract_fields(raw_ocr: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    let normalized_lines: Vec<String> = raw_ocr
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect();

    for line in &normalized_lines {
        if let Some((label, value)) = split_labeled_line(line) {
            match label.as_str() {
                "surname" | "apellidos" => {
                    fields.entry("surname".to_string()).or_insert(value.clone());
                }
                "given names" | "given name" | "name" | "nombre" => {
                    fields
                        .entry("givenNames".to_string())
                        .or_insert(value.clone());
                }
                "nationality" | "nacionalidad" => {
                    fields
                        .entry("nationality".to_string())
                        .or_insert(value.clone());
                }
                "document no" | "document number" | "numero de documento" | "id no" => {
                    fields
                        .entry("documentNumber".to_string())
                        .or_insert(value.clone());
                }
                "date of birth" | "birth date" | "fecha de nacimiento" => {
                    fields.entry("birthDate".to_string()).or_insert(value.clone());
                }
                "date of expiry" | "expiry date" | "fecha de caducidad" => {
                    fields
                        .entry("expiryDate".to_string())
                        .or_insert(value.clone());
                }
                _ => {}
            }
        }
    }

    if !fields.contains_key("fullName") {
        if let (Some(surname), Some(given_names)) =
            (fields.get("surname"), fields.get("givenNames"))
        {
            fields.insert(
                "fullName".to_string(),
                format!("{given_names} {surname}").trim().to_string(),
            );
        }
    }

    fields
}

fn run_mrz_ocr_on_image(image: &DynamicImage) -> Result<String, String> {
    let attempts = prepare_mrz_attempts(image);
    let mut best_result: Option<(i32, String)> = None;
    let mut last_error: Option<String> = None;

    let parallel_count = attempts.len().min(MRZ_PARALLEL_ATTEMPTS);
    if parallel_count > 0 {
        let mut handles = Vec::with_capacity(parallel_count);
        for attempt in attempts.iter().take(parallel_count).cloned() {
            handles.push(thread::spawn(move || run_mrz_attempt(&attempt)));
        }

        let mut parallel_outputs = Vec::with_capacity(parallel_count);
        for handle in handles {
            match handle.join() {
                Ok(output) => parallel_outputs.push(output),
                Err(_) => parallel_outputs.push(Err("MRZ attempt thread panicked".to_string())),
            }
        }

        for output in parallel_outputs {
            match output {
                Ok(raw) => {
                    if mrtd_parser::parse_mrz_text(&raw).is_some() {
                        return Ok(raw);
                    }
                    update_best_mrz_result(&mut best_result, raw);
                }
                Err(error) => last_error = Some(error),
            }
        }
    }

    for attempt in attempts.into_iter().skip(parallel_count) {
        match run_mrz_attempt(&attempt) {
            Ok(raw) => {
                if mrtd_parser::parse_mrz_text(&raw).is_some() {
                    return Ok(raw);
                }

                update_best_mrz_result(&mut best_result, raw);
            }
            Err(error) => last_error = Some(error),
        }
    }

    if let Some((_, raw)) = best_result {
        Ok(raw)
    } else if let Some(error) = last_error {
        Err(error)
    } else {
        Err("no MRZ OCR attempts were produced".to_string())
    }
}

fn update_best_mrz_result(best_result: &mut Option<(i32, String)>, raw: String) {
    let score = score_mrz_text(&raw);
    match best_result {
        Some((best_score, _)) if *best_score >= score => {}
        _ => *best_result = Some((score, raw)),
    }
}

fn decode_image_payload(image_base64: &str) -> Result<(Vec<u8>, ImageFormat), String> {
    let encoded = image_base64
        .split_once(',')
        .map(|(_, content)| content)
        .unwrap_or(image_base64);

    let bytes = STANDARD
        .decode(encoded)
        .map_err(|err| format!("invalid base64 image payload: {err}"))?;
    let format = image::guess_format(&bytes).map_err(|err| format!("invalid image: {err}"))?;
    Ok((bytes, format))
}

fn write_temp_image(bytes: &[u8], format: ImageFormat) -> Result<PathBuf, String> {
    let temp_path = temp_image_path(format)?;
    fs::write(&temp_path, bytes).map_err(|err| format!("cannot write temp image: {err}"))?;
    Ok(temp_path)
}

fn write_temp_dynamic_image(image: &DynamicImage, format: ImageFormat) -> Result<PathBuf, String> {
    let temp_path = temp_image_path(format)?;
    image
        .save_with_format(&temp_path, format)
        .map_err(|err| format!("cannot write temp image: {err}"))?;
    Ok(temp_path)
}

fn temp_image_path(format: ImageFormat) -> Result<PathBuf, String> {
    let extension = match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::WebP => "webp",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tiff",
        _ => "img",
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| format!("clock error: {err}"))?
        .as_millis();

    Ok(env::temp_dir().join(format!("leon-{timestamp}.{extension}")))
}

fn run_tesseract(image_path: &Path) -> Result<String, String> {
    run_tesseract_with_args(image_path, &["--oem", "1", "--psm", "6"])
}

fn run_tesseract_mrz_with_profile(image_path: &Path, psm: u8) -> Result<String, String> {
    let psm_value = psm.to_string();
    let mrz_args = [
        "--oem",
        "1",
        "--psm",
        psm_value.as_str(),
        "-c",
        "tessedit_char_whitelist=ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789<",
    ];

    match run_tesseract_with_args_and_lang(image_path, "ocrb", &mrz_args) {
        Ok(output) => Ok(output),
        Err(error) if is_missing_language_error(&error) => {
            run_tesseract_with_args_and_lang(image_path, "eng", &mrz_args)
        }
        Err(error) => Err(error),
    }
}

fn run_tesseract_with_args(image_path: &Path, args: &[&str]) -> Result<String, String> {
    run_tesseract_command(image_path, args)
}

fn run_tesseract_with_args_and_lang(
    image_path: &Path,
    language: &str,
    args: &[&str],
) -> Result<String, String> {
    let mut command_args = vec!["-l", language];
    command_args.extend_from_slice(args);
    run_tesseract_command(image_path, &command_args)
}

fn run_tesseract_command(image_path: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("tesseract")
        .arg(image_path)
        .arg("stdout")
        .args(args)
        .output()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                "tesseract binary not found. Install Tesseract OCR and ensure it is available in PATH.".to_string()
            } else {
                format!("failed to start tesseract: {err}")
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "tesseract finished with a non-zero exit code".to_string()
        } else {
            format!("tesseract error: {stderr}")
        });
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|err| format!("tesseract returned non-utf8 output: {err}"))?;

    Ok(stdout.trim().to_string())
}

fn is_missing_language_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("failed loading language")
        || normalized.contains("could not initialize tesseract")
        || normalized.contains("error opening data file")
}

fn prepare_mrz_attempts(image: &DynamicImage) -> Vec<MrzAttempt> {
    let mut attempts = Vec::new();
    let oriented_images = [image.clone(), image.rotate90(), image.rotate270()];

    for oriented in &oriented_images {
        for &(top_percent, bottom_percent) in &[(62, 86), (64, 88)] {
            let band = crop_mrz_focus_band(oriented, top_percent, bottom_percent);
            attempts.push(MrzAttempt {
                images: vec![preprocess_mrz_band(&band, 260, 160, 0)],
                psm: 6,
            });
        }
    }

    for oriented in &oriented_images {
        for &(top_percent, bottom_percent) in &[(62, 86), (64, 88)] {
            let band = crop_mrz_focus_band(oriented, top_percent, bottom_percent);
            attempts.push(MrzAttempt {
                images: vec![preprocess_mrz_band(&band, 300, 170, 4)],
                psm: 6,
            });
        }

        let fallback_band = crop_mrz_band(oriented, 28, 0);
        attempts.push(MrzAttempt {
            images: vec![preprocess_mrz_band(&fallback_band, 260, 160, 0)],
            psm: 6,
        });
    }

    for oriented in &oriented_images {
        let band = crop_mrz_focus_band(oriented, 62, 86);
        if let Some(split) = split_mrz_lines(&band).into_iter().next() {
            attempts.push(MrzAttempt {
                images: vec![
                    preprocess_mrz_band(&split[0], 180, 160, 0),
                    preprocess_mrz_band(&split[1], 180, 160, 0),
                ],
                psm: 7,
            });
        }
    }

    attempts
}

fn crop_mrz_band(image: &DynamicImage, height_percent: u32, upward_shift_percent: u32) -> DynamicImage {
    let (width, height) = image.dimensions();
    let band_height = ((height.saturating_mul(height_percent)).max(100) / 100)
        .max(1)
        .min(height);
    let upward_shift = height.saturating_mul(upward_shift_percent) / 100;
    let top = height
        .saturating_sub(band_height)
        .saturating_sub(upward_shift)
        .min(height.saturating_sub(1));
    let crop_height = band_height.min(height.saturating_sub(top));
    add_left_margin(&image.crop_imm(0, top, width, crop_height.max(1)), MRZ_LEFT_MARGIN_PX)
}

fn crop_mrz_focus_band(image: &DynamicImage, top_percent: u32, bottom_percent: u32) -> DynamicImage {
    let (width, height) = image.dimensions();
    let top = (height.saturating_mul(top_percent) / 100).min(height.saturating_sub(1));
    let bottom = (height.saturating_mul(bottom_percent) / 100).clamp(top + 1, height);
    add_left_margin(
        &image.crop_imm(0, top, width, bottom.saturating_sub(top).max(1)),
        MRZ_LEFT_MARGIN_PX,
    )
}

fn add_left_margin(image: &DynamicImage, margin_left: u32) -> DynamicImage {
    if margin_left == 0 {
        return image.clone();
    }

    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut widened = ImageBuffer::from_pixel(
        width.saturating_add(margin_left),
        height,
        Rgba([255, 255, 255, 255]),
    );

    for y in 0..height {
        for x in 0..width {
            let pixel = rgba.get_pixel(x, y);
            widened.put_pixel(x + margin_left, y, *pixel);
        }
    }

    DynamicImage::ImageRgba8(widened)
}

fn preprocess_mrz_band(
    band: &DynamicImage,
    min_height: u32,
    threshold: u8,
    darkening_bias: i16,
) -> DynamicImage {
    let grayscale = band.grayscale().to_luma8();
    let resized = resize_to_min_height(&grayscale, min_height);
    let thresholded = threshold_mrz_image(&resized, threshold, darkening_bias);
    DynamicImage::ImageLuma8(thresholded)
}

fn split_mrz_lines(band: &DynamicImage) -> Vec<[DynamicImage; 2]> {
    let (width, height) = band.dimensions();
    if height < 40 {
        return Vec::new();
    }

    let overlap = (height / 18).max(4);
    let upper_height = (height / 2).saturating_add(overlap).min(height);
    let lower_top = (height / 2).saturating_sub(overlap).min(height.saturating_sub(1));
    let lower_height = height.saturating_sub(lower_top).max(1);

    vec![[
        band.crop_imm(0, 0, width, upper_height.max(1)),
        band.crop_imm(0, lower_top, width, lower_height),
    ]]
}

fn resize_to_min_height(image: &GrayImage, min_height: u32) -> GrayImage {
    if image.height() >= min_height {
        return image.clone();
    }

    let scale = min_height as f32 / image.height() as f32;
    let target_width = ((image.width() as f32 * scale).round() as u32).max(1);
    image::imageops::resize(image, target_width, min_height, FilterType::CatmullRom)
}

fn threshold_mrz_image(image: &GrayImage, threshold: u8, darkening_bias: i16) -> GrayImage {
    let mut histogram = [0u32; 256];
    for pixel in image.pixels() {
        histogram[pixel[0] as usize] += 1;
    }

    let total_pixels = image.width().saturating_mul(image.height()).max(1);
    let mut weighted_sum = 0u64;
    for (level, count) in histogram.iter().enumerate() {
        weighted_sum += level as u64 * *count as u64;
    }

    let average = (weighted_sum / total_pixels as u64) as i16;
    let cutoff = (average - darkening_bias).clamp(80, 210) as u8;
    let effective_threshold = cutoff.min(threshold.max(110));

    ImageBuffer::from_fn(image.width(), image.height(), |x, y| {
        let pixel = image.get_pixel(x, y)[0];
        if pixel <= effective_threshold {
            Luma([0u8])
        } else {
            Luma([255u8])
        }
    })
}

fn score_mrz_text(raw_ocr: &str) -> i32 {
    let normalized_lines: Vec<String> = raw_ocr
        .lines()
        .map(normalize_mrz_line_for_scoring)
        .filter(|line| line.len() >= 30)
        .collect();

    let mut best_score = -10_000;
    for pair in normalized_lines.windows(2) {
        let line_1 = normalize_td3_line_length(&pair[0]);
        let line_2 = normalize_td3_line_length(&pair[1]);
        let score = score_td3_pair(&line_1, &line_2);
        if score > best_score {
            best_score = score;
        }
    }

    if best_score == -10_000 {
        normalized_lines
            .iter()
            .map(|line| {
                let normalized = normalize_td3_line_length(line);
                let mut score = 0;
                if normalized.starts_with("P<") {
                    score += 80;
                }
                score += normalized.matches('<').count() as i32 * 2;
                score -= normalized.chars().filter(|c| !MRZ_ALLOWED_CHARS.contains(*c)).count() as i32 * 10;
                score
            })
            .max()
            .unwrap_or(-10_000)
    } else {
        best_score
    }
}

fn score_td3_pair(line_1: &str, line_2: &str) -> i32 {
    let mut score = 0;
    if line_1.starts_with("P<") {
        score += 120;
    }
    if line_1.len() == 44 {
        score += 20;
    }
    if line_2.len() == 44 {
        score += 20;
    }
    if line_2[0..9].chars().any(|character| character.is_ascii_digit()) {
        score += 40;
    }
    if line_2[10..13].chars().all(|character| character.is_ascii_uppercase() || character == '<') {
        score += 25;
    }
    if line_2[13..19].chars().all(|character| character.is_ascii_digit() || character == '<') {
        score += 30;
    }
    if line_2[21..27].chars().all(|character| character.is_ascii_digit() || character == '<') {
        score += 30;
    }
    score += line_1.matches('<').count() as i32;
    score += line_2.matches('<').count() as i32;
    score -= line_1.chars().filter(|c| !MRZ_ALLOWED_CHARS.contains(*c)).count() as i32 * 10;
    score -= line_2.chars().filter(|c| !MRZ_ALLOWED_CHARS.contains(*c)).count() as i32 * 10;
    score
}

fn normalize_mrz_line_for_scoring(line: &str) -> String {
    line.trim()
        .to_ascii_uppercase()
        .chars()
        .map(|character| match character {
            'A'..='Z' | '0'..='9' | '<' => character,
            ' ' => '<',
            _ => '<',
        })
        .collect::<String>()
        .trim_matches('<')
        .to_string()
}

fn run_mrz_attempt(attempt: &MrzAttempt) -> Result<String, String> {
    let mut outputs = Vec::with_capacity(attempt.images.len());

    for image in &attempt.images {
        let temp_path = write_temp_dynamic_image(image, ImageFormat::Png)?;
        let output = run_tesseract_mrz_with_profile(&temp_path, attempt.psm);
        let _ = fs::remove_file(&temp_path);
        outputs.push(output?);
    }

    Ok(outputs.join("\n"))
}

fn normalize_td3_line_length(line: &str) -> String {
    let mut normalized = line.chars().take(44).collect::<String>();
    while normalized.len() < 44 {
        normalized.push('<');
    }
    normalized
}

fn split_labeled_line(line: &str) -> Option<(String, String)> {
    let separators = [':', ';'];

    for separator in separators {
        if let Some((left, right)) = line.split_once(separator) {
            let label = normalize_label(left);
            let value = right.trim().to_string();
            if !label.is_empty() && !value.is_empty() {
                return Some((label, value));
            }
        }
    }

    None
}

fn normalize_label(label: &str) -> String {
    label
        .trim()
        .to_ascii_lowercase()
        .replace(['.', '_'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone)]
struct MrzAttempt {
    images: Vec<DynamicImage>,
    psm: u8,
}
