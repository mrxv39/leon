use std::collections::HashMap;

use serde::Serialize;

use crate::{mrtd_parser, ocr};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractDocumentResponse {
    pub raw_ocr: String,
    pub mrz: Option<crate::mrz::MrzData>,
    pub fields: HashMap<String, String>,
}

#[tauri::command]
pub fn extract_document(image_base64: String) -> Result<ExtractDocumentResponse, String> {
    let mrz_raw = ocr::run_mrz_ocr_from_base64(&image_base64)?;
    let raw_ocr = ocr::run_ocr_from_base64(&image_base64)?;
    let mrz = mrtd_parser::parse_mrz_text(&mrz_raw);
    let mut fields = ocr::extract_fields(&raw_ocr);

    if let Some(parsed_mrz) = &mrz {
        fields
            .entry("documentNumber".to_string())
            .or_insert_with(|| parsed_mrz.document_number.clone());
        fields
            .entry("surname".to_string())
            .or_insert_with(|| parsed_mrz.surname.clone());
        fields
            .entry("givenNames".to_string())
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
            .or_insert_with(|| {
                format!("{} {}", parsed_mrz.given_names, parsed_mrz.surname)
                    .trim()
                    .to_string()
            });
    }

    Ok(ExtractDocumentResponse {
        raw_ocr,
        mrz,
        fields,
    })
}
