pub fn unescape_html(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            let mut entity = String::new();
            let mut found_semicolon = false;
            while let Some(&next_c) = chars.peek() {
                if next_c == ';' {
                    chars.next();
                    found_semicolon = true;
                    break;
                }
                if next_c.is_whitespace() || next_c == '<' || next_c == '&' || entity.len() > 10 {
                    break;
                }
                entity.push(chars.next().unwrap());
            }
            if found_semicolon {
                match entity.as_str() {
                    "nbsp" => result.push(' '),
                    "amp" => result.push('&'),
                    "lt" => result.push('<'),
                    "gt" => result.push('>'),
                    "quot" => result.push('"'),
                    "#39" | "apos" => result.push('\''),
                    _ => {
                        result.push('&');
                        result.push_str(&entity);
                        result.push(';');
                    }
                }
            } else {
                result.push('&');
                result.push_str(&entity);
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub fn html_to_text(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let mut in_skip_block = false;
    let mut tag_name = String::new();
    let mut skip_block_tag = String::new();

    for c in html.chars() {
        if c == '<' {
            in_tag = true;
            tag_name.clear();
        } else if c == '>' {
            in_tag = false;
            let base_tag = tag_name
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_lowercase();

            if in_skip_block {
                if base_tag == format!("/{}", skip_block_tag) {
                    in_skip_block = false;
                    skip_block_tag.clear();
                }
            } else if base_tag == "style" || base_tag == "script" || base_tag == "head" {
                in_skip_block = true;
                skip_block_tag = base_tag;
            } else {
                match base_tag.as_str() {
                    "p" | "div" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "br" | "li"
                    | "/p" | "/div" | "/tr" | "/h1" | "/h2" | "/h3" | "/h4" | "/h5" | "/h6"
                    | "/li" => {
                        result.push('\n');
                    }
                    _ => {}
                }
            }
        } else if in_tag {
            if (c == '/' && tag_name.is_empty())
                || c.is_ascii_alphanumeric()
                || c == '-'
                || c == '_'
            {
                tag_name.push(c);
            } else if c.is_whitespace() && !tag_name.contains(' ') {
                tag_name.push(' ');
            }
        } else if !in_skip_block {
            result.push(c);
        }
    }

    unescape_html(&result)
}

pub fn clean_whitespace(input: &str) -> String {
    let mut result = String::new();
    let mut last_was_newline = false;

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !last_was_newline && !result.is_empty() {
                result.push_str("\n\n");
                last_was_newline = true;
            }
        } else {
            if !result.is_empty() && !last_was_newline {
                result.push('\n');
            }
            result.push_str(trimmed);
            last_was_newline = false;
        }
    }
    result.trim().to_string()
}

pub fn url_decode(input: &str) -> String {
    let mut result = String::new();
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(val) = u8::from_str_radix(&hex, 16) {
                result.push(val as char);
            } else {
                result.push('%');
                result.push_str(&hex);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

#[allow(clippy::manual_is_multiple_of)]
fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn date_to_seconds(date_str: &str) -> Option<u64> {
    if date_str.len() < 19 {
        return None;
    }
    let year = date_str[0..4].parse::<u64>().ok()?;
    let month = date_str[5..7].parse::<u64>().ok()?;
    let day = date_str[8..10].parse::<u64>().ok()?;
    let hour = date_str[11..13].parse::<u64>().ok()?;
    let min = date_str[14..16].parse::<u64>().ok()?;
    let sec = date_str[17..19].parse::<u64>().ok()?;

    let days_in_months = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

    let mut days = 0;
    for y in 2000..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }

    for m in 1..month {
        if m == 2 && is_leap_year(year) {
            days += 29;
        } else {
            days += days_in_months[m as usize];
        }
    }

    days += day - 1;

    let total_seconds = days * 86400 + hour * 3600 + min * 60 + sec;
    Some(total_seconds)
}

fn format_remaining(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    let seconds = secs % 60;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{}d", days));
    }
    if hours > 0 || days > 0 {
        parts.push(format!("{}h", hours));
    }
    if mins > 0 || hours > 0 || days > 0 {
        parts.push(format!("{}m", mins));
    }
    parts.push(format!("{}s", seconds));

    parts.join(" ")
}

pub fn get_remaining_time(ted: &str, konec: &str) -> String {
    if let (Some(t_secs), Some(k_secs)) = (date_to_seconds(ted), date_to_seconds(konec)) {
        if k_secs > t_secs {
            format_remaining(k_secs - t_secs)
        } else {
            "Expired".to_string()
        }
    } else {
        "Unknown".to_string()
    }
}

pub fn parse_duration(input: &str) -> Result<u64, String> {
    let cleaned = input.trim().to_lowercase();
    match cleaned.as_str() {
        "10m" | "+10m" | "10min" | "+10min" => Ok(4200),
        "1d" | "1day" => Ok(86400),
        "3d" | "3days" => Ok(259200),
        "5d" | "5days" => Ok(432000),
        "1w" | "1week" => Ok(604800),
        "2w" | "2weeks" => Ok(1209600),
        other => {
            if let Ok(secs) = other.parse::<u64>() {
                Ok(secs)
            } else {
                Err(format!(
                    "Invalid duration '{}'. Supported: 10m, 1d, 3d, 5d, 1w, 2w, or raw seconds.",
                    input
                ))
            }
        }
    }
}
