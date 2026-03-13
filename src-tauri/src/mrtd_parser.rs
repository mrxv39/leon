use mrtd::{parse, parse_without_checks, Document, Gender};

use crate::mrz::MrzData;

pub fn parse_mrz_text(text: &str) -> Option<MrzData> {
    let normalized_lines: Vec<String> = text
        .lines()
        .map(normalize_ocr_line)
        .filter(|line| line.len() >= 24)
        .collect();

    let mut best_candidate: Option<(i32, MrzData)> = None;

    for pair in normalized_lines.windows(2) {
        let line_1 = normalize_td3_line_1(&pair[0]);
        if !line_1.starts_with("P<") {
            continue;
        }

        for line_2 in candidate_line_2_variants(&pair[1]) {
            let candidate_text = format!("{line_1}{line_2}");
            let parsed = match parse(&candidate_text).or_else(|_| parse_without_checks(&candidate_text)) {
                Ok(parsed) => parsed,
                Err(_) => continue,
            };

            let Some(mapped) = map_document(parsed) else {
                continue;
            };
            let score = score_candidate(&line_1, &line_2, &mapped);

            match &best_candidate {
                Some((best_score, _)) if *best_score >= score => {}
                _ => best_candidate = Some((score, mapped)),
            }
        }
    }

    best_candidate.map(|(_, mapped)| mapped)
}

fn map_document(document: Document) -> Option<MrzData> {
    match document {
        Document::Passport(passport) => Some(MrzData {
            document_number: passport.passport_number.trim_matches('<').to_string(),
            surname: passport.surnames.join(" ").trim().to_string(),
            given_names: passport.given_names.join(" ").trim().to_string(),
            nationality: passport.nationality.trim_matches('<').to_string(),
            birth_date: passport.birth_date.format("%Y-%m-%d").to_string(),
            sex: map_gender(passport.gender),
            expiry_date: passport.expiry_date.format("%Y-%m-%d").to_string(),
        }),
        Document::IdentityCard(_) => None,
    }
}

fn map_gender(gender: Gender) -> String {
    match gender {
        Gender::Male => "M".to_string(),
        Gender::Female => "F".to_string(),
        Gender::Other => "X".to_string(),
    }
}

fn normalize_ocr_line(line: &str) -> String {
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

fn normalize_td3_line_1(line: &str) -> String {
    let mut normalized = String::with_capacity(44);

    for (index, character) in line.chars().enumerate().take(44) {
        let repaired = if index == 0 {
            repair_alpha(character)
        } else if index < 5 {
            repair_alpha_or_filler(character)
        } else {
            repair_alpha_or_filler(character)
        };
        normalized.push(repaired);
    }

    while normalized.len() < 44 {
        normalized.push('<');
    }

    if !normalized.starts_with("P<") {
        normalized.replace_range(0..2, "P<");
    }

    normalized
}

fn normalize_td3_line_2(line: &str) -> String {
    let mut normalized = String::with_capacity(44);

    for (index, character) in line.chars().enumerate().take(44) {
        let repaired = match index {
            9 | 13..=19 | 21..=27 | 42..=43 => repair_digit_or_filler(character),
            10..=12 => repair_alpha_or_filler(character),
            20 => repair_sex(character),
            _ => repair_alnum_or_filler(character),
        };
        normalized.push(repaired);
    }

    while normalized.len() < 44 {
        normalized.push('<');
    }

    normalized
}

fn candidate_line_2_variants(line: &str) -> Vec<String> {
    let normalized = normalize_td3_line_2(line);
    let mut candidates = vec![normalized];
    let raw_len = line.chars().take(44).count();

    for missing_prefix in 1..=(44usize.saturating_sub(raw_len).min(2)) {
        for prefix in inferred_prefixes() {
            let mut shifted = prefix.to_string().repeat(missing_prefix);
            shifted.push_str(line);
            candidates.push(normalize_td3_line_2(&shifted));
        }
    }

    candidates.sort();
    candidates.dedup();
    candidates
}

fn inferred_prefixes() -> Vec<char> {
    let mut prefixes = vec!['<', 'P'];
    for character in 'A'..='Z' {
        if character != 'P' {
            prefixes.push(character);
        }
    }
    for character in '0'..='9' {
        prefixes.push(character);
    }
    prefixes
}

fn repair_alpha(character: char) -> char {
    match character {
        '0' => 'O',
        '1' => 'I',
        '2' => 'Z',
        '4' => 'A',
        '5' => 'S',
        '6' => 'G',
        '8' => 'B',
        '9' => 'G',
        'A'..='Z' => character,
        _ => '<',
    }
}

fn repair_alpha_or_filler(character: char) -> char {
    match character {
        '<' => '<',
        other => repair_alpha(other),
    }
}

fn repair_digit_or_filler(character: char) -> char {
    match character {
        '<' => '<',
        'O' | 'Q' | 'D' | 'U' => '0',
        'I' | 'L' | 'T' => '1',
        'Z' => '2',
        'A' => '4',
        'S' => '5',
        'G' => '6',
        'B' => '8',
        '0'..='9' => character,
        _ => '<',
    }
}

fn repair_alnum_or_filler(character: char) -> char {
    match character {
        'A'..='Z' | '0'..='9' | '<' => character,
        _ => '<',
    }
}

fn repair_sex(character: char) -> char {
    match character {
        'M' | 'F' | '<' => character,
        'X' => '<',
        other => repair_alpha_or_filler(other),
    }
}

fn score_candidate(line_1: &str, line_2: &str, parsed: &MrzData) -> i32 {
    let mut score = 0;

    if line_1.starts_with("P<") {
        score += 100;
    }
    if !parsed.document_number.is_empty() {
        score += 50;
    }
    if !parsed.surname.is_empty() {
        score += 30;
    }
    if !parsed.given_names.is_empty() {
        score += 30;
    }
    if parsed.nationality.len() == 3 {
        score += 20;
    }
    if matches!(parsed.sex.as_str(), "M" | "F" | "X") {
        score += 10;
    }
    score += line_1.matches('<').count() as i32;
    score += line_2.matches('<').count() as i32;
    score
}

#[cfg(test)]
mod tests {
    use super::parse_mrz_text;

    #[test]
    fn parses_known_td3_example() {
        let text = "P<UTOERIKSSON<<ANNA<MARIA<<<<<<<<<<<<<<<<<<<\nL898902C36UTO7408122F1204159ZE184226B<<<<<10";
        let parsed = parse_mrz_text(text).expect("expected valid TD3");
        assert_eq!(parsed.document_number, "L898902C3");
        assert_eq!(parsed.nationality, "UTO");
    }

    #[test]
    fn parses_nearly_valid_ocr_text_with_missing_prefix() {
        let text = "P<ESPVILLAFAINA<MARTINEZ<<JAVIER<<<<<<<<<<<<\nACO764426ESP8408196M2602165A4778641400<<<34";
        let parsed = parse_mrz_text(text).expect("expected repaired TD3");
        assert_eq!(parsed.nationality, "ESP");
    }
}
