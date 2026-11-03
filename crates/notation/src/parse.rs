//! Minimal KDL-subset parser for Framesmith composition files.
//!
//! Sprint 1 needs only a small shape: a `composition` root node with
//! `key=value` properties and flat child element nodes inside `{ ... }`.
//! Anything outside that shape is reported as a located parse diagnostic
//! instead of being guessed at.

/// A parsed scalar value with its 1-based source location.
#[derive(Debug, Clone)]
pub struct RawValue {
    pub kind: ValueKind,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub enum ValueKind {
    Str(String),
    Int(i64),
    Float(f64),
    /// Exact rational written bare as `num/den` (e.g. fps `30000/1001`).
    Ratio(u64, u64),
    Bool(bool),
}

/// A parsed node: `name key=value ...` with optional `{ children }`.
#[derive(Debug, Clone)]
pub struct RawNode {
    pub name: String,
    pub name_line: usize,
    pub name_column: usize,
    pub props: Vec<(String, RawValue)>,
    pub children: Vec<RawNode>,
}

/// A located parse failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

/// Split one line into whitespace-separated tokens. Quoting is tracked
/// across the whole token, so `key="a value with spaces"` stays one
/// token. Returns each token with its 1-based byte column. An
/// unterminated quote reports the column where the string started.
fn tokenize(line_text: &str) -> Result<Vec<(String, usize)>, usize> {
    let bytes = line_text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let col = i + 1;
        let mut tok = String::new();
        let mut in_str = false;
        let mut str_col = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if in_str {
                if b == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                    tok.push('"');
                    i += 2;
                } else if b == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                    tok.push('\\');
                    i += 2;
                } else if b == b'"' {
                    tok.push('"');
                    in_str = false;
                    i += 1;
                } else {
                    // Byte-oriented pushes would shred multi-byte UTF-8
                    // (emoji arrives as four bytes, not four Latin-1
                    // characters): decode the character properly.
                    let ch = line_text[i..].chars().next().unwrap_or('\u{fffd}');
                    tok.push(ch);
                    i += ch.len_utf8();
                }
            } else if b == b'"' {
                in_str = true;
                if str_col == 0 {
                    str_col = i + 1;
                }
                tok.push('"');
                i += 1;
            } else if b == b' ' || b == b'\t' {
                break;
            } else {
                let ch = line_text[i..].chars().next().unwrap_or('\u{fffd}');
                tok.push(ch);
                i += ch.len_utf8();
            }
        }
        if in_str {
            return Err(str_col);
        }
        out.push((tok, col));
    }
    Ok(out)
}

/// Parse the value half of a `key=value` token. A `"..."` span (with
/// `\"` and `\\` escapes, already boundary-checked by the tokenizer)
/// becomes a string; anything else is a bare scalar.
fn parse_value(token: &str, line: usize, column: usize) -> Result<RawValue, ParseError> {
    let fail = || ParseError {
        line,
        column,
        message: format!("invalid value {token:?}"),
    };
    if let Some(inner) = token
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .filter(|_| token.len() >= 2)
    {
        return Ok(RawValue {
            kind: ValueKind::Str(inner.to_string()),
            line,
            column,
        });
    }
    if token.starts_with('"') {
        return Err(fail());
    }
    match token {
        "true" => {
            return Ok(RawValue {
                kind: ValueKind::Bool(true),
                line,
                column,
            })
        }
        "false" => {
            return Ok(RawValue {
                kind: ValueKind::Bool(false),
                line,
                column,
            })
        }
        _ => {}
    }
    if let Some((a, b)) = token.split_once('/') {
        let num: u64 = a.parse().map_err(|_| fail())?;
        let den: u64 = b.parse().map_err(|_| fail())?;
        return Ok(RawValue {
            kind: ValueKind::Ratio(num, den),
            line,
            column,
        });
    }
    if let Ok(n) = token.parse::<i64>() {
        // Reject float-looking spellings that happen to parse downstream.
        if token.contains('.') || token.contains('e') || token.contains('E') {
            return Err(fail());
        }
        return Ok(RawValue {
            kind: ValueKind::Int(n),
            line,
            column,
        });
    }
    if token.parse::<f64>().is_ok() {
        return Ok(RawValue {
            kind: ValueKind::Float(token.parse::<f64>().map_err(|_| fail())?),
            line,
            column,
        });
    }
    Err(fail())
}

/// Parse a whole composition file into top-level nodes.
pub fn parse_document(text: &str) -> Result<Vec<RawNode>, ParseError> {
    let mut roots: Vec<RawNode> = Vec::new();
    // Stack of open blocks; each entry holds that block's child nodes.
    let mut stack: Vec<(RawNode, usize)> = Vec::new();

    for (idx, raw_line) in text.lines().enumerate() {
        let line_no = idx + 1;
        // Strip `//` comments (only outside quotes: scan for // while
        // tracking quote state).
        let mut cut = raw_line.len();
        let mut in_str = false;
        let mut prev_backslash = false;
        let lb = raw_line.as_bytes();
        let mut k = 0;
        while k + 1 < lb.len() {
            if in_str {
                if prev_backslash {
                    prev_backslash = false;
                } else if lb[k] == b'\\' {
                    prev_backslash = true;
                } else if lb[k] == b'"' {
                    in_str = false;
                }
            } else if lb[k] == b'"' {
                in_str = true;
            } else if lb[k] == b'/' && lb[k + 1] == b'/' {
                cut = k;
                break;
            }
            k += 1;
        }
        let line = raw_line[..cut].trim();
        if line.is_empty() {
            continue;
        }
        if line == "}" {
            let (node, _) = stack.pop().ok_or(ParseError {
                line: line_no,
                column: 1,
                message: "unexpected `}` with no open block".to_string(),
            })?;
            match stack.last_mut() {
                Some((parent, _)) => parent.children.push(node),
                None => roots.push(node),
            }
            continue;
        }
        let (head, opens_block) = match line.strip_suffix('{') {
            Some(h) => (h.trim_end(), true),
            None => (line, false),
        };
        if head.is_empty() {
            return Err(ParseError {
                line: line_no,
                column: 1,
                message: "expected a node before `{`".to_string(),
            });
        }
        let tokens = tokenize(head).map_err(|col| ParseError {
            line: line_no,
            column: col,
            message: "unterminated string".to_string(),
        })?;
        if tokens.is_empty() {
            continue;
        }
        let (name, name_col) = &tokens[0];
        if name.contains('=') || name == "{" || name == "}" {
            return Err(ParseError {
                line: line_no,
                column: *name_col,
                message: format!("invalid node name {name:?}"),
            });
        }
        let mut props = Vec::new();
        for (tok, col) in &tokens[1..] {
            let (k, v) = tok.split_once('=').ok_or(ParseError {
                line: line_no,
                column: *col,
                message: format!("expected `key=value`, found positional argument {tok:?}"),
            })?;
            if k.is_empty() {
                return Err(ParseError {
                    line: line_no,
                    column: *col,
                    message: "empty property name".to_string(),
                });
            }
            // Strip the quote marker before parsing: `"30"` stays a string.
            let value = parse_value(v, line_no, *col + k.len() + 1)?;
            props.push((k.to_string(), value));
        }
        let node = RawNode {
            name: name.clone(),
            name_line: line_no,
            name_column: *name_col,
            props,
            children: Vec::new(),
        };
        // Column of the `{` is only used for unclosed-block errors.
        let brace_col = line.len();
        if opens_block {
            stack.push((node, line_no));
            let _ = brace_col;
        } else {
            match stack.last_mut() {
                Some((parent, _)) => parent.children.push(node),
                None => roots.push(node),
            }
        }
    }

    if let Some((_, open_line)) = stack.pop() {
        return Err(ParseError {
            line: open_line,
            column: 1,
            message: "unclosed `{` block".to_string(),
        });
    }
    Ok(roots)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(src: &str) -> Vec<RawNode> {
        parse_document(src).expect("should parse")
    }

    #[test]
    fn root_with_child_block() {
        let nodes = parse_ok(
            "composition name=\"hi\" width=1280 height=720 duration=3 fps=30 background=\"#000\" version=1 {\n    text value=\"Hey\" start=0 duration=3\n}\n",
        );
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "composition");
        assert_eq!(nodes[0].children.len(), 1);
        assert_eq!(nodes[0].children[0].name, "text");
    }

    #[test]
    fn utf8_values_survive_tokenizing() {
        // Multi-byte characters must pass through whole: the old
        // byte-push shredded emoji into Latin-1 mojibake.
        let nodes =
            parse_ok("composition name=\"t\" {\n    text value=\"A😀Z\" start=0 duration=1\n}\n");
        let (_, v) = nodes[0].children[0]
            .props
            .iter()
            .find(|(k, _)| k == "value")
            .unwrap();
        match v {
            RawValue {
                kind: ValueKind::Str(s),
                ..
            } => assert_eq!(s, "A😀Z"),
            other => panic!("expected a string, got {other:?}"),
        }
    }

    #[test]
    fn value_shapes() {
        let nodes = parse_ok("composition fps=30 g=30000/1001 f=29.97 s=\"x\" b=true {\n}\n");
        let kinds: Vec<String> = nodes[0]
            .props
            .iter()
            .map(|(_, v)| format!("{:?}", v.kind))
            .collect();
        assert!(kinds[0].starts_with("Int"), "{kinds:?}");
        assert!(kinds[1].starts_with("Ratio"), "{kinds:?}");
        assert!(kinds[2].starts_with("Float"), "{kinds:?}");
        assert!(kinds[3].starts_with("Str"), "{kinds:?}");
        assert!(kinds[4].starts_with("Bool"), "{kinds:?}");
    }

    #[test]
    fn locations_are_reported() {
        let err =
            parse_document("composition name=\"x\" {\n    text value=\"oops\n}\n").unwrap_err();
        assert_eq!(err.line, 2);
    }

    #[test]
    fn unclosed_block_reports_open_line() {
        let err = parse_document("composition name=\"x\" {\n    text value=\"hi\"\n").unwrap_err();
        assert_eq!(err.line, 1);
        assert!(err.message.contains("unclosed"));
    }

    #[test]
    fn stray_close_brace_is_located() {
        let err = parse_document("}\n").unwrap_err();
        assert_eq!((err.line, err.column), (1, 1));
    }

    #[test]
    fn comments_are_ignored() {
        let nodes = parse_ok("// hello\ncomposition name=\"x\" { // trailing\n}\n");
        assert_eq!(nodes.len(), 1);
    }

    #[test]
    fn positional_arg_is_rejected() {
        assert!(parse_document("composition \"oops\" {\n}\n").is_err());
    }
}
