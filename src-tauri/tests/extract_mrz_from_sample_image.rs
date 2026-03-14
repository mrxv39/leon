use std::path::PathBuf;

use leon_lib::{mrtd_parser, ocr};

#[test]
fn extracts_expected_mrz_data_from_sample_image() {
    // Keep IMG_4061 (1).jpg at the repo root to run this integration test locally.
    let image_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../IMG_4061 (1).jpg");
    if !image_path.exists() {
        return;
    }

    let mrz_text =
        ocr::run_mrz_ocr_on_path(&image_path).expect("expected MRZ OCR to succeed on sample image");
    let parsed = mrtd_parser::parse_mrz_text(&mrz_text);
    let mrz = parsed.expect("expected MRZ parser to extract passport data from sample image");

    assert_eq!(mrz.surname, "VILLAFAINA MARTINEZ");
    assert_eq!(mrz.given_names, "JAVIER");
    assert_eq!(mrz.nationality, "ESP");
    assert_eq!(mrz.birth_date, "1984-08-19");
    assert_eq!(mrz.sex, "M");
    assert_eq!(mrz.expiry_date, "2026-02-16");
    assert!(
        matches!(mrz.document_number.as_str(), "PAC076442" | "ACO76442"),
        "unexpected document number from OCR: {}",
        mrz.document_number
    );
}
