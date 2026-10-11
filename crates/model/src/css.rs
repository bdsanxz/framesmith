//! A scanner for style sheets and style attributes. It finds declarations,
//! at-rules and file references with the line each is on. It does not
//! work out which rule applies to which element.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    /// Lower case.
    pub property: String,
    pub value: String,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtRule {
    /// Lower case, without the `@`.
    pub name: String,
    pub prelude: String,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UrlRef {
    pub url: String,
    pub line: u32,
    /// Named by `@import`: another style sheet.
    pub import: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sheet {
    pub declarations: Vec<Declaration>,
    pub at_rules: Vec<AtRule>,
    pub urls: Vec<UrlRef>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum End {
    Semicolon,
    Open,
    Close,
}

/// Scans a style sheet, or the value of a style attribute. Lines count from 1.
pub fn scan(text: &str) -> Sheet {
    let mut sheet = Sheet::default();
    let bytes = text.as_bytes();
    let mut pos = 0;
    let mut line: u32 = 1;
    // The statement being collected, with comments left out.
    let mut statement = String::new();
    let mut statement_line: u32 = 1;
    let mut depth = 0usize;
    while pos < bytes.len() {
        let b = bytes[pos];
        if b == b'/' && bytes.get(pos + 1) == Some(&b'*') {
            let end = text[pos + 2..].find("*/").map_or(bytes.len(), |e| pos + 2 + e + 2);
            line += count_lines(&text[pos..end]);
            pos = end;
            continue;
        }
        if statement.is_empty() {
            if b.is_ascii_whitespace() {
                if b == b'\n' {
                    line += 1;
                }
                pos += 1;
                continue;
            }
            statement_line = line;
        }
        match b {
            b'"' | b'\'' => {
                let mut end = pos + 1;
                while end < bytes.len() && bytes[end] != b {
                    end += if bytes[end] == b'\\' { 2 } else { 1 };
                }
                let end = (end + 1).min(bytes.len());
                let end = (end..=bytes.len()).find(|&e| text.is_char_boundary(e)).unwrap_or(bytes.len());
                statement.push_str(&text[pos..end]);
                line += count_lines(&text[pos..end]);
                pos = end;
                continue;
            }
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b';' | b'{' | b'}' if depth == 0 => {
                let end = match b {
                    b';' => End::Semicolon,
                    b'{' => End::Open,
                    _ => End::Close,
                };
                finish(&mut sheet, &statement, statement_line, end);
                statement.clear();
                pos += 1;
                continue;
            }
            b'\n' => line += 1,
            _ => {}
        }
        // Copy one whole character.
        let end = (pos + 1..=bytes.len()).find(|&e| text.is_char_boundary(e)).unwrap_or(bytes.len());
        statement.push_str(&text[pos..end]);
        pos = end;
    }
    finish(&mut sheet, &statement, statement_line, End::Close);
    sheet
}

fn count_lines(text: &str) -> u32 {
    text.bytes().filter(|&b| b == b'\n').count() as u32
}

fn finish(sheet: &mut Sheet, statement: &str, line: u32, end: End) {
    let text = statement.trim_end();
    if text.is_empty() {
        return;
    }
    if let Some(rule) = text.strip_prefix('@') {
        let name_end = rule.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).unwrap_or(rule.len());
        let name = rule[..name_end].to_ascii_lowercase();
        let prelude = rule[name_end..].trim().to_string();
        if name == "import" {
            if let Some(url) = import_target(&prelude) {
                sheet.urls.push(UrlRef { url, line, import: true });
            }
        } else {
            collect_urls(sheet, text, line);
        }
        sheet.at_rules.push(AtRule { name, prelude, line });
        return;
    }
    if end == End::Open {
        // A selector: nothing to record.
        return;
    }
    let Some((property, value)) = text.split_once(':') else {
        return;
    };
    collect_urls(sheet, text, line);
    sheet.declarations.push(Declaration {
        property: property.trim().to_ascii_lowercase(),
        value: value.trim().to_string(),
        line,
    });
}

fn unquote(text: &str) -> &str {
    let text = text.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = text.strip_prefix(quote).and_then(|t| t.strip_suffix(quote)) {
            return inner;
        }
    }
    text
}

fn import_target(prelude: &str) -> Option<String> {
    let first = prelude.chars().next()?;
    if first == '"' || first == '\'' {
        let end = prelude[1..].find(first)?;
        return Some(prelude[1..=end].to_string());
    }
    let lower = prelude.to_ascii_lowercase();
    if lower.starts_with("url(") {
        let end = prelude.find(')')?;
        return Some(unquote(&prelude[4..end]).to_string());
    }
    None
}

fn collect_urls(sheet: &mut Sheet, text: &str, line: u32) {
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        match bytes[pos] {
            // Text inside a string names nothing.
            quote @ (b'"' | b'\'') => {
                pos += 1;
                while pos < bytes.len() && bytes[pos] != quote {
                    pos += if bytes[pos] == b'\\' { 2 } else { 1 };
                }
                pos += 1;
            }
            // `url(` must not be the tail of a longer name.
            b'u' if lower[pos..].starts_with("url(") && !(pos > 0 && is_name_byte(bytes[pos - 1])) => {
                let Some(close) = text[pos..].find(')') else {
                    return;
                };
                let url = unquote(&text[pos + 4..pos + close]).to_string();
                if !url.is_empty() {
                    sheet.urls.push(UrlRef {
                        url,
                        line: line + count_lines(&text[..pos]),
                        import: false,
                    });
                }
                pos += close + 1;
            }
            _ => pos += 1,
        }
    }
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// The value of a declaration without a trailing `!important`, in lower case.
pub fn plain_value(value: &str) -> String {
    let lower = value.trim().to_ascii_lowercase();
    match lower.rfind('!') {
        Some(bang) if lower[bang + 1..].trim() == "important" => lower[..bang].trim().to_string(),
        _ => lower,
    }
}
