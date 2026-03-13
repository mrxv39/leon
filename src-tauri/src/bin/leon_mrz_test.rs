use std::{env, path::PathBuf};

use leon_lib::{mrtd_parser, ocr};

fn main() {
    let image_path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("IMG_4061 (1).jpg"));

    match ocr::run_mrz_ocr_on_path(&image_path) {
        Ok(raw) => {
            println!("MRZ raw:");
            println!("{raw}");
            println!();

            let lines = ocr::extract_mrz_candidate_lines(&raw);
            if lines.is_empty() {
                println!("Detected lines: none");
            } else {
                println!("Detected lines:");
                for (index, line) in lines.iter().enumerate() {
                    println!("{}: {}", index + 1, line);
                }
            }
            println!();

            match mrtd_parser::parse_mrz_text(&raw) {
                Some(parsed) => println!("parse_mrz_text: {:?}", parsed),
                None => println!("parse_mrz_text: None"),
            }
        }
        Err(error) => {
            eprintln!("MRZ test failed: {error}");
            std::process::exit(1);
        }
    }
}
