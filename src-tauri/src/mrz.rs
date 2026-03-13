use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MrzData {
    pub document_number: String,
    pub surname: String,
    pub given_names: String,
    pub nationality: String,
    pub birth_date: String,
    pub sex: String,
    pub expiry_date: String,
}

pub fn parse_td3(raw_ocr: &str) -> Option<MrzData> {
    let normalized_lines: Vec<String> = raw_ocr
        .lines()
        .map(normalize_mrz_line)
        .filter(|line| line.len() >= 20)
        .collect();

    let mut best_candidate: Option<(i32, MrzData)> = None;

    for pair in normalized_lines.windows(2) {
        if let Some((score, parsed)) = parse_td3_candidate_pair(&pair[0], &pair[1]) {
            match &best_candidate {
                Some((best_score, _)) if *best_score >= score => {}
                _ => best_candidate = Some((score, parsed)),
            }
        }
    }

    best_candidate.map(|(_, parsed)| parsed)
}

fn normalize_mrz_line(line: &str) -> String {
    line.trim()
        .to_ascii_uppercase()
        .chars()
        .map(|character| {
            if character.is_ascii_uppercase() || character.is_ascii_digit() || character == '<' {
                character
            } else {
                '<'
            }
        })
        .collect()
}

fn normalize_td3_length(line: &str) -> String {
    let mut normalized = line.chars().take(44).collect::<String>();
    while normalized.len() < 44 {
        normalized.push('<');
    }
    normalized
}

fn cleanup_name(value: &str) -> String {
    value
        .replace('<', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

enum DateKind {
    Birth,
    Expiry,
}

fn format_mrz_date(raw: &str, date_kind: DateKind) -> String {
    if raw.len() != 6 || !raw.chars().all(|character| character.is_ascii_digit()) {
        return raw.to_string();
    }

    let year = raw[0..2].parse::<u32>().unwrap_or_default();
    let month = &raw[2..4];
    let day = &raw[4..6];

    let full_year = match date_kind {
        DateKind::Birth => {
            if year > 26 {
                1900 + year
            } else {
                2000 + year
            }
        }
        DateKind::Expiry => {
            if year < 70 {
                2000 + year
            } else {
                1900 + year
            }
        }
    };

    format!("{full_year:04}-{month}-{day}")
}

fn normalize_sex(raw: &str) -> String {
    match raw {
        "M" => "M".to_string(),
        "F" => "F".to_string(),
        "<" => "X".to_string(),
        value => value.to_string(),
    }
}

fn parse_td3_candidate_pair(line_1_raw: &str, line_2_raw: &str) -> Option<(i32, MrzData)> {
    let line_1 = normalize_td3_length(line_1_raw);
    let line_2 = normalize_td3_length(line_2_raw);

    if !line_1.starts_with("P<") {
        return None;
    }

    let names_raw = &line_1[5..44];
    let mut name_parts = names_raw.split("<<");
    let surname = cleanup_name(name_parts.next().unwrap_or_default());
    let given_names = cleanup_name(name_parts.next().unwrap_or_default());

    let document_number = line_2[0..9].replace('<', "").trim().to_string();
    let nationality = line_2[10..13].replace('<', "").trim().to_string();
    let birth_date = format_mrz_date(&line_2[13..19], DateKind::Birth);
    let sex = normalize_sex(&line_2[20..21]);
    let expiry_date = format_mrz_date(&line_2[21..27], DateKind::Expiry);

    let mut score = 0;
    score += 100;
    score += line_1.matches('<').count() as i32;
    score += line_2.matches('<').count() as i32;

    if !surname.is_empty() {
        score += 25;
    }
    if !given_names.is_empty() {
        score += 25;
    }
    if document_number.len() >= 6 {
        score += 35;
    }
    if nationality.len() == 3 {
        score += 20;
    }
    if line_2[13..19].chars().all(|character| character.is_ascii_digit() || character == '<') {
        score += 20;
    }
    if line_2[21..27].chars().all(|character| character.is_ascii_digit() || character == '<') {
        score += 20;
    }
    if matches!(sex.as_str(), "M" | "F" | "X") {
        score += 10;
    }

    Some((
        score,
        MrzData {
            document_number,
            surname,
            given_names,
            nationality,
            birth_date,
            sex,
            expiry_date,
        },
    ))
}
