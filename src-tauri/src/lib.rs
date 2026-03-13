mod commands;
pub mod mrz;
pub mod mrtd_parser;
pub mod ocr;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![commands::extract_document])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
