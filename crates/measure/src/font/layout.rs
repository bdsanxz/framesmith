//! The layout table of the figure variants, as `docs/spec/test-font.md`, section 7 sets out: a GSUB table with two
//! features, `onum` and `tnum`. Each is one single substitution, from the lining figure of a digit to its variant, under
//! the default script (`DFLT`). The features `lnum` and `pnum` have no lookup: the default figures are the lining figures
//! and the proportional ones, so the font has nothing for them to reach.

/// The glyph of U+0030, the first digit. The glyphs of U+0020 to U+007E come in code order from glyph 1, so the glyph of a
/// code is the code less 0x1F.
pub const FIRST_DIGIT: u16 = 17;
/// The glyph of the tabular figure of the digit 0; the tabular figures follow the 95 glyphs of the printable characters.
pub const TABULAR: u16 = 96;
/// The glyph of the old-style figure of the digit 0; the old-style figures follow the tabular ones.
pub const OLDSTYLE: u16 = 106;

/// The features and their substitutions, in tag order, as `(tag, pairs)` where each pair is (lining glyph, variant glyph).
pub fn figure_features() -> Vec<([u8; 4], Vec<(u16, u16)>)> {
    let digits = |first: u16| (0..10u16).map(|digit| (FIRST_DIGIT + digit, first + digit)).collect::<Vec<_>>();
    vec![(*b"onum", digits(OLDSTYLE)), (*b"tnum", digits(TABULAR))]
}

/// The GSUB table: a header, a script list, a feature list and a lookup list, in that order, each offset from the start of
/// the table or of its own list. The feature list holds the features in the order given, which is tag order.
pub fn gsub(features: &[([u8; 4], Vec<(u16, u16)>)]) -> Vec<u8> {
    let script_list = script_list(features.len());
    let feature_list = feature_list(features);
    let lookup_list = lookup_list(features);
    let script_at = 10;
    let feature_at = script_at + script_list.len();
    let lookup_at = feature_at + feature_list.len();
    let mut out = u16s(&[1, 0, script_at as u16, feature_at as u16, lookup_at as u16]);
    out.extend(script_list);
    out.extend(feature_list);
    out.extend(lookup_list);
    out
}

/// One script, DFLT, whose default language system asks for every feature, in order.
fn script_list(features: usize) -> Vec<u8> {
    let indices: Vec<u16> = (0..features as u16).collect();
    // The script table: the default language system at 4 bytes from its start, no other language system.
    let mut script = u16s(&[4, 0]);
    // The language system: no lookup order, no required feature, then the feature indices.
    script.extend(u16s(&[0, 0xFFFF, features as u16]));
    script.extend(u16s(&indices));
    let mut out = u16s(&[1]);
    out.extend(b"DFLT");
    out.extend(u16s(&[8]));
    out.extend(script);
    out
}

/// The feature records, then the feature tables: each table names the one lookup of its feature.
fn feature_list(features: &[([u8; 4], Vec<(u16, u16)>)]) -> Vec<u8> {
    let tables: Vec<Vec<u8>> = (0..features.len()).map(|index| u16s(&[0, 1, index as u16])).collect();
    let mut out = u16s(&[features.len() as u16]);
    let mut offset = 2 + 6 * features.len();
    for ((tag, _), table) in features.iter().zip(&tables) {
        out.extend(tag);
        out.extend(u16s(&[offset as u16]));
        offset += table.len();
    }
    for table in tables {
        out.extend(table);
    }
    out
}

/// The lookups, one for each feature, in the same order: a count, the offset of each lookup, then the lookups.
fn lookup_list(features: &[([u8; 4], Vec<(u16, u16)>)]) -> Vec<u8> {
    let lookups: Vec<Vec<u8>> = features.iter().map(|(_, pairs)| lookup(pairs)).collect();
    let mut out = u16s(&[lookups.len() as u16]);
    let mut offset = 2 + 2 * lookups.len();
    for lookup in &lookups {
        out.extend(u16s(&[offset as u16]));
        offset += lookup.len();
    }
    for lookup in lookups {
        out.extend(lookup);
    }
    out
}

/// A lookup of type 1, single substitution, with one subtable of format 2 and its coverage of format 1. The coverage lists
/// the glyphs to replace in ascending order, and the substitutes follow in the same order.
fn lookup(pairs: &[(u16, u16)]) -> Vec<u8> {
    let mut sorted = pairs.to_vec();
    sorted.sort_unstable();
    let count = sorted.len() as u16;
    // The lookup: its type, its flags, one subtable, which starts 8 bytes in.
    let mut out = u16s(&[1, 0, 1, 8]);
    // The subtable: its format, the offset of the coverage (after the substitutes), the count and the substitutes.
    out.extend(u16s(&[2, 6 + 2 * count, count]));
    out.extend(u16s(&sorted.iter().map(|pair| pair.1).collect::<Vec<_>>()));
    // The coverage: its format, the count and the glyphs.
    out.extend(u16s(&[1, count]));
    out.extend(u16s(&sorted.iter().map(|pair| pair.0).collect::<Vec<_>>()));
    out
}

/// Big-endian 16-bit values.
fn u16s(values: &[u16]) -> Vec<u8> {
    values.iter().flat_map(|value| value.to_be_bytes()).collect()
}
