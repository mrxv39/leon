use std::collections::HashMap;

use serde::Serialize;

use crate::{mrtd_parser, ocr};

const COMMON_SECOND_SURNAMES: [&str; 10] = [
    "MARTINEZ",
    "GARCIA",
    "RODRIGUEZ",
    "GONZALEZ",
    "LOPEZ",
    "SANCHEZ",
    "FERNANDEZ",
    "PEREZ",
    "MARTIN",
    "GOMEZ",
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractDocumentResponse {
    pub raw_ocr: String,
    pub mrz: Option<crate::mrz::MrzData>,
    pub fields: HashMap<String, String>,
    pub warnings: Vec<String>,
}

fn normalize_mrz_name_fields(mrz: &mut crate::mrz::MrzData) {
    mrz.surname = normalize_surname(&mrz.surname);
    mrz.given_names = normalize_given_names(&mrz.given_names);
}

fn normalize_surname(surname: &str) -> String {
    let trimmed = surname.trim();
    if trimmed.contains(' ') || trimmed.len() <= 8 {
        return trimmed.to_string();
    }

    let matches: Vec<&str> = COMMON_SECOND_SURNAMES
        .iter()
        .copied()
        .filter(|candidate| {
            trimmed.ends_with(candidate) && trimmed.len() > candidate.len() + 1
        })
        .collect();

    if matches.len() != 1 {
        return trimmed.to_string();
    }

    let second = matches[0];
    let split_at = trimmed.len() - second.len();
    format!("{} {}", &trimmed[..split_at], second)
}

fn normalize_given_names(given_names: &str) -> String {
    let mut normalized = given_names.trim().to_string();

    for suffix in ["SK K", "SK", "K"] {
        if let Some(prefix) = normalized.strip_suffix(suffix) {
            let candidate = prefix.trim_end();
            if candidate.len() >= 3 {
                normalized = candidate.to_string();
                break;
            }
        }
    }

    if let Some((prefix, trailing)) = normalized.rsplit_once(' ') {
        let trailing = trailing.trim();
        if (1..=2).contains(&trailing.len()) && prefix.split_whitespace().count() >= 1 {
            return prefix.trim_end().to_string();
        }
    }

    normalized
}

#[tauri::command]
pub async fn extract_document(image_base64: String) -> Result<ExtractDocumentResponse, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mrz_raw = ocr::run_mrz_ocr_from_base64(&image_base64)?;
        let raw_ocr = ocr::run_ocr_from_base64(&image_base64)?;
        let mut mrz = mrtd_parser::parse_mrz_text(&mrz_raw);
        let mut fields = ocr::extract_fields(&raw_ocr);
        let mut warnings = Vec::new();

        if let Some(parsed_mrz) = &mut mrz {
            normalize_mrz_name_fields(parsed_mrz);
            let full_name = format!("{} {}", parsed_mrz.given_names, parsed_mrz.surname)
                .trim()
                .to_string();

            fields
                .entry("documentNumber".to_string())
                .or_insert_with(|| parsed_mrz.document_number.clone());
            fields
                .entry("surname".to_string())
                .and_modify(|value| *value = parsed_mrz.surname.clone())
                .or_insert_with(|| parsed_mrz.surname.clone());
            fields
                .entry("givenNames".to_string())
                .and_modify(|value| *value = parsed_mrz.given_names.clone())
                .or_insert_with(|| parsed_mrz.given_names.clone());
            fields
                .entry("nationality".to_string())
                .or_insert_with(|| parsed_mrz.nationality.clone());
            fields
                .entry("birthDate".to_string())
                .or_insert_with(|| parsed_mrz.birth_date.clone());
            fields
                .entry("sex".to_string())
                .or_insert_with(|| parsed_mrz.sex.clone());
            fields
                .entry("expiryDate".to_string())
                .or_insert_with(|| parsed_mrz.expiry_date.clone());
            fields
                .entry("documentType".to_string())
                .or_insert_with(|| "Passport TD3".to_string());
            fields
                .entry("fullName".to_string())
                .and_modify(|value| *value = full_name.clone())
                .or_insert(full_name);

            if !parsed_mrz.document_number.is_empty()
                && !parsed_mrz.document_number.starts_with('P')
                && parsed_mrz.document_number.len() == 8
            {
                warnings.push(
                    "El numero de documento podria ser incorrecto (en pasaportes suele comenzar por P). Revise el valor extraido.".to_string(),
                );
            }
        }

        Ok(ExtractDocumentResponse {
            raw_ocr,
            mrz,
            fields,
            warnings,
        })
    })
    .await
    .map_err(|error| format!("extract_document task failed: {error}"))?
}
