pub fn unescape_html(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '&' {
            result.push(c);
            continue;
        }

        let mut entity = String::new();
        let mut found_semicolon = false;
        while let Some(&next_c) = chars.peek() {
            if next_c == ';' {
                chars.next();
                found_semicolon = true;
                break;
            }
            if next_c.is_whitespace() || next_c == '<' || next_c == '&' || entity.len() > 32 {
                break;
            }
            if let Some(next) = chars.next() {
                entity.push(next);
            }
        }

        if found_semicolon {
            if let Some(decoded) = decode_html_entity(&entity) {
                result.push(decoded);
            } else {
                result.push('&');
                result.push_str(&entity);
                result.push(';');
            }
        } else {
            result.push('&');
            result.push_str(&entity);
        }
    }

    result
}

fn decode_html_entity(entity: &str) -> Option<char> {
    match entity {
        "nbsp" => Some(' '),
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => decode_numeric_entity(entity),
    }
}

fn decode_numeric_entity(entity: &str) -> Option<char> {
    let value = if let Some(hex) = entity
        .strip_prefix("#x")
        .or_else(|| entity.strip_prefix("#X"))
    {
        u32::from_str_radix(hex, 16).ok()?
    } else if let Some(decimal) = entity.strip_prefix('#') {
        decimal.parse::<u32>().ok()?
    } else {
        return None;
    };

    char::from_u32(value)
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
                if base_tag.strip_prefix('/') == Some(skip_block_tag.as_str()) {
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
    let mut result = String::with_capacity(input.len());
    let mut percent_bytes = Vec::new();
    let mut index = 0;

    while index < input.len() {
        let byte = input.as_bytes()[index];
        if byte == b'%' && index + 2 < input.len() {
            let high = input.as_bytes()[index + 1];
            let low = input.as_bytes()[index + 2];
            if let (Some(high), Some(low)) = (from_hex(high), from_hex(low)) {
                percent_bytes.push(high << 4 | low);
                index += 3;
                continue;
            }
        }

        flush_percent_bytes(&mut result, &mut percent_bytes);

        if byte == b'+' {
            result.push(' ');
            index += 1;
        } else if let Some(next) = input[index..].chars().next() {
            result.push(next);
            index += next.len_utf8();
        } else {
            break;
        }
    }

    flush_percent_bytes(&mut result, &mut percent_bytes);
    result
}

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn flush_percent_bytes(result: &mut String, bytes: &mut Vec<u8>) {
    if bytes.is_empty() {
        return;
    }

    result.push_str(&String::from_utf8_lossy(bytes));
    bytes.clear();
}

#[allow(clippy::manual_is_multiple_of)]
fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn date_to_seconds(date_str: &str) -> Option<u64> {
    if date_str.get(4..5)? != "-"
        || date_str.get(7..8)? != "-"
        || date_str.get(10..11)? != " "
        || date_str.get(13..14)? != ":"
        || date_str.get(16..17)? != ":"
    {
        return None;
    }

    let year = parse_range(date_str, 0, 4)?;
    let month = parse_range(date_str, 5, 7)?;
    let day = parse_range(date_str, 8, 10)?;
    let hour = parse_range(date_str, 11, 13)?;
    let min = parse_range(date_str, 14, 16)?;
    let sec = parse_range(date_str, 17, 19)?;

    if !(1..=12).contains(&month) || hour > 23 || min > 59 || sec > 59 {
        return None;
    }

    let day_limit = days_in_month(year, month)?;
    if day == 0 || day > day_limit {
        return None;
    }

    let mut days = 0;
    for y in 2000..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }

    for m in 1..month {
        days += days_in_month(year, m)?;
    }

    days += day - 1;

    Some(days * 86400 + hour * 3600 + min * 60 + sec)
}

fn parse_range(input: &str, start: usize, end: usize) -> Option<u64> {
    input.get(start..end)?.parse::<u64>().ok()
}

fn days_in_month(year: u64, month: u64) -> Option<u64> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 if is_leap_year(year) => Some(29),
        2 => Some(28),
        _ => None,
    }
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
    let cleaned = cleaned.strip_prefix('+').unwrap_or(&cleaned);

    match cleaned {
        "10m" | "10min" => Ok(600),
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_duration ---

    #[test]
    fn parse_duration_known_aliases() {
        assert_eq!(parse_duration("10m"), Ok(600));
        assert_eq!(parse_duration("10min"), Ok(600));
        assert_eq!(parse_duration("1d"), Ok(86400));
        assert_eq!(parse_duration("1day"), Ok(86400));
        assert_eq!(parse_duration("3d"), Ok(259200));
        assert_eq!(parse_duration("3days"), Ok(259200));
        assert_eq!(parse_duration("5d"), Ok(432000));
        assert_eq!(parse_duration("5days"), Ok(432000));
        assert_eq!(parse_duration("1w"), Ok(604800));
        assert_eq!(parse_duration("1week"), Ok(604800));
        assert_eq!(parse_duration("2w"), Ok(1209600));
        assert_eq!(parse_duration("2weeks"), Ok(1209600));
    }

    #[test]
    fn parse_duration_plus_prefix_and_case() {
        assert_eq!(parse_duration("+10m"), Ok(600));
        assert_eq!(parse_duration("+1d"), Ok(86400));
        assert_eq!(parse_duration("1D"), Ok(86400));
        assert_eq!(parse_duration("1W"), Ok(604800));
    }

    #[test]
    fn parse_duration_whitespace_trimmed() {
        assert_eq!(parse_duration("  10m  "), Ok(600));
        assert_eq!(parse_duration("\t1d\n"), Ok(86400));
    }

    #[test]
    fn parse_duration_raw_seconds() {
        assert_eq!(parse_duration("0"), Ok(0));
        assert_eq!(parse_duration("1"), Ok(1));
        assert_eq!(parse_duration("3600"), Ok(3600));
        assert_eq!(parse_duration("999999"), Ok(999999));
    }

    #[test]
    fn parse_duration_invalid_rejected() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("xyz").is_err());
        assert!(parse_duration("10x").is_err());
        assert!(parse_duration("-10m").is_err());
    }

    // --- unescape_html ---

    #[test]
    fn unescape_html_named_entities() {
        assert_eq!(unescape_html("&amp;"), "&");
        assert_eq!(unescape_html("&lt;"), "<");
        assert_eq!(unescape_html("&gt;"), ">");
        assert_eq!(unescape_html("&quot;"), "\"");
        assert_eq!(unescape_html("&apos;"), "'");
        assert_eq!(unescape_html("&nbsp;"), " ");
    }

    #[test]
    fn unescape_html_numeric_decimal() {
        assert_eq!(unescape_html("&#60;"), "<");
        assert_eq!(unescape_html("&#65;"), "A");
        assert_eq!(unescape_html("&#8364;"), "€");
    }

    #[test]
    fn unescape_html_numeric_hex() {
        assert_eq!(unescape_html("&#x3C;"), "<");
        assert_eq!(unescape_html("&#x41;"), "A");
        assert_eq!(unescape_html("&#x20AC;"), "€");
        assert_eq!(unescape_html("&#X3C;"), "<");
    }

    #[test]
    fn unescape_html_unknown_entity_preserved() {
        assert_eq!(unescape_html("&unknown;"), "&unknown;");
        assert_eq!(unescape_html("&bogus;"), "&bogus;");
    }

    #[test]
    fn unescape_html_missing_semicolon() {
        assert_eq!(unescape_html("&amp"), "&amp");
    }

    #[test]
    fn unescape_html_entity_too_long_truncated() {
        let long = "a".repeat(33);
        let input = format!("&{};", long);
        // entity >32 chars: loop breaks before consuming ';', so ';' is literal output
        assert_eq!(unescape_html(&input), input);
    }

    #[test]
    fn unescape_html_mixed_content() {
        assert_eq!(unescape_html("a &amp; b"), "a & b");
        assert_eq!(unescape_html("&lt;tag&gt;"), "<tag>");
        assert_eq!(unescape_html("&amp;nbsp;"), "&nbsp;");
    }

    #[test]
    fn unescape_html_empty_and_no_entities() {
        assert_eq!(unescape_html(""), "");
        assert_eq!(unescape_html("hello"), "hello");
    }

    #[test]
    fn unescape_html_amp_at_end() {
        assert_eq!(unescape_html("&"), "&");
        assert_eq!(unescape_html("foo&"), "foo&");
    }

    #[test]
    fn unescape_html_break_on_special_chars() {
        // '&' inside potential entity causes a break
        assert_eq!(unescape_html("&a&amp;"), "&a&");
        // '<' inside potential entity causes a break
        assert_eq!(unescape_html("&a<tag>"), "&a<tag>");
    }

    // --- url_decode ---

    #[test]
    fn url_decode_simple_percent() {
        assert_eq!(url_decode("%20"), " ");
        assert_eq!(url_decode("%21"), "!");
        assert_eq!(url_decode("%7E"), "~");
    }

    #[test]
    fn url_decode_plus_to_space() {
        assert_eq!(url_decode("+"), " ");
        assert_eq!(url_decode("a+b"), "a b");
        assert_eq!(url_decode("hello+world"), "hello world");
    }

    #[test]
    fn url_decode_utf8_sequences() {
        assert_eq!(url_decode("%E2%82%AC"), "€");
        assert_eq!(url_decode("%F0%9F%94%92"), "🔒");
    }

    #[test]
    fn url_decode_invalid_percent_preserved() {
        assert_eq!(url_decode("%ZZ"), "%ZZ");
        assert_eq!(url_decode("%2"), "%2");
        assert_eq!(url_decode("%"), "%");
    }

    #[test]
    fn url_decode_mixed() {
        assert_eq!(url_decode("a%20b%20c"), "a b c");
        assert_eq!(url_decode("hello%2C+world"), "hello, world");
    }

    #[test]
    fn url_decode_empty_and_no_encoding() {
        assert_eq!(url_decode(""), "");
        assert_eq!(url_decode("hello"), "hello");
        assert_eq!(url_decode("abc123"), "abc123");
    }

    // --- html_to_text ---

    #[test]
    fn html_to_text_strips_tags() {
        assert_eq!(html_to_text("<b>bold</b>"), "bold");
        assert_eq!(html_to_text("<i>italic</i>"), "italic");
        assert_eq!(html_to_text("<a href=\"x\">link</a>"), "link");
    }

    #[test]
    fn html_to_text_block_tags_add_newlines() {
        // Both opening and closing block tags produce a newline
        assert_eq!(html_to_text("<p>a</p><p>b</p>"), "\na\n\nb\n");
        assert_eq!(html_to_text("<div>x</div><div>y</div>"), "\nx\n\ny\n");
    }

    #[test]
    fn html_to_text_skips_script_style_head() {
        assert_eq!(html_to_text("<script>var x=1;</script>keep"), "keep");
        assert_eq!(html_to_text("<style>body{}</style>text"), "text");
        assert_eq!(html_to_text("<head><title>x</title></head>body"), "body");
    }

    #[test]
    fn html_to_text_nested_skip_block() {
        let html = "<script>if (a < b) {}</script>keep";
        assert_eq!(html_to_text(html), "keep");
    }

    #[test]
    fn html_to_text_self_closing_br_li() {
        assert_eq!(html_to_text("hello<br>world"), "hello\nworld");
        assert_eq!(html_to_text("item1<li>item2"), "item1\nitem2");
    }

    #[test]
    fn html_to_text_entities_decoded() {
        assert_eq!(html_to_text("&amp;"), "&");
        assert_eq!(html_to_text("&lt;br&gt;"), "<br>");
    }

    #[test]
    fn html_to_text_empty_and_no_html() {
        assert_eq!(html_to_text(""), "");
        assert_eq!(html_to_text("plain text"), "plain text");
    }

    // --- clean_whitespace ---

    #[test]
    fn clean_whitespace_basic_trim() {
        assert_eq!(clean_whitespace("  hello  "), "hello");
        assert_eq!(clean_whitespace("hello"), "hello");
        assert_eq!(clean_whitespace(""), "");
    }

    #[test]
    fn clean_whitespace_lines_collapsed() {
        assert_eq!(clean_whitespace("  first  \n  second  "), "first\nsecond");
    }

    #[test]
    fn clean_whitespace_blank_lines_collapsed() {
        assert_eq!(clean_whitespace("a\n\n\n\nb"), "a\n\nb");
        assert_eq!(clean_whitespace("a\n\nb\n\nc"), "a\n\nb\n\nc");
    }

    #[test]
    fn clean_whitespace_leading_trailing_blanks_removed() {
        assert_eq!(clean_whitespace("\n\n\na\n\nb\n\n\n"), "a\n\nb");
    }

    #[test]
    fn clean_whitespace_all_blank() {
        assert_eq!(clean_whitespace("  \n  \n  "), "");
        assert_eq!(clean_whitespace("\n\n\n"), "");
    }

    #[test]
    fn clean_whitespace_single_paragraph() {
        assert_eq!(clean_whitespace("  hello world  \n  \n  "), "hello world");
    }

    // --- get_remaining_time ---

    #[test]
    fn get_remaining_time_future() {
        let result = get_remaining_time("2025-01-01 00:00:00", "2025-01-02 00:00:00");
        assert_eq!(result, "1d 0h 0m 0s");
    }

    #[test]
    fn get_remaining_time_expired() {
        let result = get_remaining_time("2025-01-02 00:00:00", "2025-01-01 00:00:00");
        assert_eq!(result, "Expired");
    }

    #[test]
    fn get_remaining_time_same_time() {
        let result = get_remaining_time("2025-01-01 00:00:00", "2025-01-01 00:00:00");
        assert_eq!(result, "Expired");
    }

    #[test]
    fn get_remaining_time_invalid_dates() {
        assert_eq!(
            get_remaining_time("invalid", "2025-01-02 00:00:00"),
            "Unknown"
        );
        assert_eq!(
            get_remaining_time("2025-01-01 00:00:00", "invalid"),
            "Unknown"
        );
        assert_eq!(get_remaining_time("", ""), "Unknown");
    }

    #[test]
    fn get_remaining_time_out_of_range_values() {
        // month > 12
        assert_eq!(
            get_remaining_time("2025-13-01 00:00:00", "2025-01-02 00:00:00"),
            "Unknown"
        );
        // day > days_in_month
        assert_eq!(
            get_remaining_time("2025-01-32 00:00:00", "2025-01-02 00:00:00"),
            "Unknown"
        );
        // hour > 23
        assert_eq!(
            get_remaining_time("2025-01-01 24:00:00", "2025-01-02 00:00:00"),
            "Unknown"
        );
    }

    #[test]
    fn get_remaining_time_small_diffs() {
        // just seconds
        assert_eq!(
            get_remaining_time("2025-01-01 00:00:00", "2025-01-01 00:00:30"),
            "30s"
        );
        // minutes
        assert_eq!(
            get_remaining_time("2025-01-01 00:00:00", "2025-01-01 01:00:00"),
            "1h 0m 0s"
        );
    }
}
