//! SVG-subset path travel (Sprint 7 motion, CAP-008): parsing,
//! arc-length tables, tangent headings, and same-structure morphs.
//!
//! A position tween or key names a `path` (commands `M`, `L`, `C`,
//! `Q`, `A`, `Z`, absolute or relative); the clip sits at the eased
//! distance along the true curve length, and `orient=true` rotates
//! it to the path tangent. Morphing between two same-structure
//! paths interpolates command numbers. Everything here is a pure
//! function of its inputs, so re-renders are byte-identical.

use std::fmt;

/// One path command in absolute coordinates: parsing normalizes
/// relative forms here, so `l` and `L` share one structure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    Move {
        to: [f64; 2],
    },
    Line {
        to: [f64; 2],
    },
    Cubic {
        c1: [f64; 2],
        c2: [f64; 2],
        to: [f64; 2],
    },
    Quad {
        c: [f64; 2],
        to: [f64; 2],
    },
    Arc {
        rx: f64,
        ry: f64,
        rot: f64,
        large: bool,
        sweep: bool,
        to: [f64; 2],
    },
    Close {
        to: [f64; 2],
    },
}

impl Cmd {
    /// The normalized command letter, for structure checks and
    /// diagnostics: `M`, `L`, `C`, `Q`, `A`, or `Z`.
    pub fn letter(self) -> char {
        match self {
            Cmd::Move { .. } => 'M',
            Cmd::Line { .. } => 'L',
            Cmd::Cubic { .. } => 'C',
            Cmd::Quad { .. } => 'Q',
            Cmd::Arc { .. } => 'A',
            Cmd::Close { .. } => 'Z',
        }
    }
}

/// A parsed path: commands in order, in absolute coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub cmds: Vec<Cmd>,
}

/// Why a path string is unparseable: always tied to the command in
/// context (when one is), so diagnostics can name it.
#[derive(Clone, Debug, PartialEq)]
pub struct PathError {
    /// Uppercase command letter in context (`M`, `L`, `C`, `Q`, `A`,
    /// `Z`), or `None` when no command was open.
    pub command: Option<char>,
    pub kind: ErrorKind,
}

/// How a path string fails to parse.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ErrorKind {
    /// No commands at all (empty or blank data).
    Empty,
    /// A letter outside the `M`/`L`/`C`/`Q`/`A`/`Z` subset.
    UnknownCommand(char),
    /// A number that is missing, malformed, or non-finite.
    BadNumber,
    /// Data ends in the middle of a command's numbers.
    Truncated,
    /// An arc flag that is not `0` or `1`.
    BadFlag,
    /// A stray number with no open command.
    StrayNumber,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::Empty => write!(
                f,
                "names no commands; path commands are `M`, `L`, `C`, `Q`, `A`, and `Z`"
            ),
            ErrorKind::UnknownCommand(c) => write!(
                f,
                "names unsupported command `{c}`; path commands are `M`, `L`, `C`, `Q`, `A`, and `Z`"
            ),
            ErrorKind::BadNumber => match self.command {
                Some(c) => write!(f, "command `{c}` has a bad number; every coordinate is a plain number"),
                None => write!(f, "has a bad number; every coordinate is a plain number"),
            },
            ErrorKind::Truncated => match self.command {
                Some(c) => write!(
                    f,
                    "ends in the middle of command `{c}`; every command carries its full numbers"
                ),
                None => write!(f, "ends mid-command; every command carries its full numbers"),
            },
            ErrorKind::BadFlag => write!(
                f,
                "command `A` has a bad arc flag; the large-arc and sweep flags are `0` or `1`"
            ),
            ErrorKind::StrayNumber => write!(
                f,
                "opens with a number; paths open with a command (`M`, `L`, `C`, `Q`, `A`, or `Z`)"
            ),
        }
    }
}

/// Why two paths cannot morph: the first structural difference, so
/// diagnostics can name the command.
#[derive(Clone, Debug, PartialEq)]
pub struct MorphError {
    /// 1-based command index of the first difference.
    pub index: usize,
    /// Command letter in the first path (`None` when the first path
    /// has no command there).
    pub from_cmd: Option<char>,
    /// Command letter in the second path (`None` when the second
    /// path has no command there).
    pub to_cmd: Option<char>,
    /// Whether the difference is the command itself or only the arc
    /// flags (which never interpolate).
    pub flags: bool,
}

impl fmt::Display for MorphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.flags {
            return write!(
                f,
                "command {} is `A` in both paths but the arc flags differ; morphs interpolate numbers, never flags",
                self.index
            );
        }
        match (self.from_cmd, self.to_cmd) {
            (Some(a), Some(b)) => write!(
                f,
                "command {} is `{a}` in the first path but `{b}` in the second; morphs need the same command structure",
                self.index
            ),
            (Some(a), None) => write!(
                f,
                "command {} is `{a}` in the first path but missing in the second; morphs need the same command structure",
                self.index
            ),
            (None, Some(b)) => write!(
                f,
                "command {} is missing in the first path but `{b}` in the second; morphs need the same command structure",
                self.index
            ),
            (None, None) => write!(f, "paths differ in structure; morphs need the same command structure"),
        }
    }
}

/// Accepted command letters, for diagnostics.
pub const ACCEPTED: &str = "`M`, `L`, `C`, `Q`, `A`, and `Z`";

/// Whether `c` opens a command: any ASCII letter. Letters outside
/// the `M`/`L`/`C`/`Q`/`A`/`Z` subset fail as unsupported, naming
/// the letter, rather than misreading as a number.
fn is_command(c: u8) -> bool {
    c.is_ascii_alphabetic()
}

/// Skip separators (whitespace and commas) from `i`.
fn skip_sep(data: &[u8], mut i: usize) -> usize {
    while i < data.len() && (data[i].is_ascii_whitespace() || data[i] == b',') {
        i += 1;
    }
    i
}

/// Scan one SVG number from `i`: the end index plus the value, or
/// `None` when no well-formed number opens there.
fn scan_number(data: &[u8], i: usize) -> Option<(usize, f64)> {
    let mut j = i;
    if j < data.len() && (data[j] == b'+' || data[j] == b'-') {
        j += 1;
    }
    let int_start = j;
    while j < data.len() && data[j].is_ascii_digit() {
        j += 1;
    }
    let mut frac_digits = 0;
    if j < data.len() && data[j] == b'.' {
        j += 1;
        let frac_start = j;
        while j < data.len() && data[j].is_ascii_digit() {
            j += 1;
        }
        frac_digits = j - frac_start;
    }
    if j == int_start && frac_digits == 0 {
        return None;
    }
    if j < data.len() && (data[j] == b'e' || data[j] == b'E') {
        let mut k = j + 1;
        if k < data.len() && (data[k] == b'+' || data[k] == b'-') {
            k += 1;
        }
        let exp_start = k;
        while k < data.len() && data[k].is_ascii_digit() {
            k += 1;
        }
        if k == exp_start {
            return None;
        }
        j = k;
    }
    let text = std::str::from_utf8(&data[i..j]).ok()?;
    let value: f64 = text.parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    Some((j, value))
}

/// Scan one arc flag (`0` or `1`) from `i`.
fn scan_flag(data: &[u8], i: usize) -> Option<(usize, bool)> {
    let i = skip_sep(data, i);
    if i < data.len() && (data[i] == b'0' || data[i] == b'1') {
        Some((i + 1, data[i] == b'1'))
    } else {
        None
    }
}

/// Next number of the open command: separators skipped, then one
/// number, else the failure for `command`.
fn next_number(data: &[u8], i: usize, command: char) -> Result<(usize, f64), PathError> {
    let at = skip_sep(data, i);
    if at >= data.len() {
        return Err(PathError {
            command: Some(command),
            kind: ErrorKind::Truncated,
        });
    }
    if is_command(data[at]) {
        return Err(PathError {
            command: Some(command),
            kind: ErrorKind::Truncated,
        });
    }
    scan_number(data, at).ok_or(PathError {
        command: Some(command),
        kind: ErrorKind::BadNumber,
    })
}

/// Read one (x, y) pair of the open command.
fn next_pair(data: &[u8], i: usize, command: char) -> Result<(usize, [f64; 2]), PathError> {
    let (j, x) = next_number(data, i, command)?;
    let (k, y) = next_number(data, j, command)?;
    Ok((k, [x, y]))
}

/// Parse SVG-subset path data (`M`/`L`/`C`/`Q`/`A`/`Z`, absolute or
/// relative) into absolute commands. Implicit repeated parameter
/// groups repeat the open command (pairs after `M` run as `L`).
pub fn parse(data: &str) -> Result<Path, PathError> {
    parse_impl(data).map_err(|(error, _)| error)
}

/// A path parse failure with the 1-based index of the failing
/// command (Sprint 8 look, CAP-005/T005): the count of commands that
/// parsed before the failure, plus one, so diagnostics name the
/// command to fix. An empty-data failure reports index 1.
#[derive(Clone, Debug, PartialEq)]
pub struct IndexedError {
    pub error: PathError,
    pub index: usize,
}

impl fmt::Display for IndexedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.error.kind {
            ErrorKind::Empty => write!(f, "{}", self.error),
            _ => match self.error.command {
                Some(c) => write!(f, "command {} (`{c}`) {}", self.index, self.error),
                None => write!(f, "command {} {}", self.index, self.error),
            },
        }
    }
}

/// Parse path data like [`parse`], keeping the 1-based index of the
/// failing command for diagnostics.
pub fn parse_indexed(data: &str) -> Result<Path, IndexedError> {
    parse_impl(data).map_err(|(error, index)| IndexedError { error, index })
}

fn parse_impl(data: &str) -> Result<Path, (PathError, usize)> {
    let bytes = data.as_bytes();
    let mut i = skip_sep(bytes, 0);
    let mut cmds: Vec<Cmd> = Vec::new();
    // Pen state: the current point and the subpath start (`Z` draws
    // back to it).
    let (mut cur, mut sub_start) = ([0.0, 0.0], [0.0, 0.0]);
    // The open command for implicit repeats: its letter plus whether
    // it opened relative. Pairs after `M` run as `L`.
    let mut open: Option<(char, bool)> = None;
    if i < bytes.len() && !is_command(bytes[i]) {
        return Err((
            PathError {
                command: None,
                kind: ErrorKind::StrayNumber,
            },
            cmds.len() + 1,
        ));
    }
    while i < bytes.len() {
        i = skip_sep(bytes, i);
        if i >= bytes.len() {
            break;
        }
        let raw = bytes[i];
        if is_command(raw) {
            let upper = raw.to_ascii_uppercase() as char;
            if !matches!(upper, 'M' | 'L' | 'C' | 'Q' | 'A' | 'Z') {
                return Err((
                    PathError {
                        command: None,
                        kind: ErrorKind::UnknownCommand(upper),
                    },
                    cmds.len() + 1,
                ));
            }
            i += 1;
            let relative = raw.is_ascii_lowercase();
            match upper {
                'M' => {
                    let (j, pair) = next_pair(bytes, i, 'M').map_err(|e| (e, cmds.len() + 1))?;
                    i = j;
                    let to = if relative {
                        [cur[0] + pair[0], cur[1] + pair[1]]
                    } else {
                        pair
                    };
                    cur = to;
                    sub_start = to;
                    cmds.push(Cmd::Move { to });
                    open = Some(('L', relative));
                    // Pairs after `M` run as implicit `L` (same case).
                    loop {
                        let at = skip_sep(bytes, i);
                        if at >= bytes.len() || is_command(bytes[at]) {
                            i = at;
                            break;
                        }
                        i = parse_group(bytes, i, 'L', relative, &mut cur, &mut cmds)?;
                    }
                }
                'Z' => {
                    cmds.push(Cmd::Close { to: sub_start });
                    cur = sub_start;
                }
                _ => {
                    open = Some((upper, relative));
                    i = parse_curve(bytes, i, upper, relative, &mut cur, &mut cmds)?;
                }
            }
        } else if let Some((cmd, relative)) = open {
            // Implicit repeat of the open command (same case).
            i = parse_group(bytes, i, cmd, relative, &mut cur, &mut cmds)?;
        } else {
            return Err((
                PathError {
                    command: None,
                    kind: ErrorKind::StrayNumber,
                },
                cmds.len() + 1,
            ));
        }
    }
    if cmds.is_empty() {
        return Err((
            PathError {
                command: None,
                kind: ErrorKind::Empty,
            },
            1,
        ));
    }
    Ok(Path { cmds })
}

/// Parse one explicit command group, then its implicit repeats
/// (same letter, same case).
fn parse_curve(
    bytes: &[u8],
    i: usize,
    cmd: char,
    relative: bool,
    cur: &mut [f64; 2],
    cmds: &mut Vec<Cmd>,
) -> Result<usize, (PathError, usize)> {
    let mut i = parse_group(bytes, i, cmd, relative, cur, cmds)?;
    loop {
        let at = skip_sep(bytes, i);
        if at >= bytes.len() || is_command(bytes[at]) {
            return Ok(at);
        }
        i = parse_group(bytes, i, cmd, relative, cur, cmds)?;
    }
}

/// Parse one parameter group of `cmd`, pushing the absolute command.
/// `A` reads flags strictly (`0`/`1`); `C` reads three pairs, `Q`
/// two, `A` reads radii, rotation, flags, and the endpoint.
fn parse_group(
    bytes: &[u8],
    i: usize,
    cmd: char,
    relative: bool,
    cur: &mut [f64; 2],
    cmds: &mut Vec<Cmd>,
) -> Result<usize, (PathError, usize)> {
    // Every failure below aborts the command at `cmds.len() + 1`:
    // the failing command has not been pushed yet.
    let at_fail = |e: PathError| (e, cmds.len() + 1);
    let shift = |p: [f64; 2]| -> [f64; 2] {
        if relative {
            [cur[0] + p[0], cur[1] + p[1]]
        } else {
            p
        }
    };
    match cmd {
        'L' => {
            let (j, pair) = next_pair(bytes, i, 'L').map_err(at_fail)?;
            let to = shift(pair);
            *cur = to;
            cmds.push(Cmd::Line { to });
            Ok(j)
        }
        'C' => {
            let (mut j, p1) = next_pair(bytes, i, 'C').map_err(at_fail)?;
            let (k, p2);
            (k, p2) = next_pair(bytes, j, 'C').map_err(at_fail)?;
            j = k;
            let (k, p3);
            (k, p3) = next_pair(bytes, j, 'C').map_err(at_fail)?;
            j = k;
            let (c1, c2, to) = (shift(p1), shift(p2), shift(p3));
            *cur = to;
            cmds.push(Cmd::Cubic { c1, c2, to });
            Ok(j)
        }
        'Q' => {
            let (mut j, p1) = next_pair(bytes, i, 'Q').map_err(at_fail)?;
            let (k, p2);
            (k, p2) = next_pair(bytes, j, 'Q').map_err(at_fail)?;
            j = k;
            let (c, to) = (shift(p1), shift(p2));
            *cur = to;
            cmds.push(Cmd::Quad { c, to });
            Ok(j)
        }
        'A' => {
            let (mut j, radii) = next_pair(bytes, i, 'A').map_err(at_fail)?;
            let (k, rot);
            (k, rot) = next_number(bytes, j, 'A').map_err(at_fail)?;
            j = k;
            let at = skip_sep(bytes, j);
            if at >= bytes.len() {
                return Err(at_fail(PathError {
                    command: Some('A'),
                    kind: ErrorKind::Truncated,
                }));
            }
            if is_command(bytes[at]) {
                return Err(at_fail(PathError {
                    command: Some('A'),
                    kind: ErrorKind::Truncated,
                }));
            }
            let (k, large) = scan_flag(bytes, j)
                .ok_or(PathError {
                    command: Some('A'),
                    kind: ErrorKind::BadFlag,
                })
                .map_err(at_fail)?;
            j = k;
            let (k, sweep) = scan_flag(bytes, j)
                .ok_or(PathError {
                    command: Some('A'),
                    kind: ErrorKind::BadFlag,
                })
                .map_err(at_fail)?;
            j = k;
            let (k, pair) = next_pair(bytes, j, 'A').map_err(at_fail)?;
            j = k;
            let to = shift(pair);
            let (rx, ry) = (radii[0].abs(), radii[1].abs());
            *cur = to;
            cmds.push(Cmd::Arc {
                rx,
                ry,
                rot,
                large,
                sweep,
                to,
            });
            Ok(j)
        }
        _ => Err(at_fail(PathError {
            command: Some(cmd),
            kind: ErrorKind::Truncated,
        })),
    }
}

/// The command letters of a path in order: the morph structure.
pub fn structure(path: &Path) -> Vec<char> {
    path.cmds.iter().map(|cmd| cmd.letter()).collect()
}

/// Whether two paths share the command structure a morph needs:
/// the same command letters in the same order.
pub fn same_structure(a: &Path, b: &Path) -> bool {
    structure(a) == structure(b)
}

/// Interpolate two same-structure paths at `p` (0 = `a`, 1 = `b`):
/// every command number lerps. Arc flags must match exactly — flags
/// never interpolate — and any structural difference fails naming
/// the first differing command.
pub fn morph(a: &Path, b: &Path, p: f64) -> Result<Path, MorphError> {
    if a.cmds.len() != b.cmds.len() {
        let n = a.cmds.len().min(b.cmds.len());
        for (i, (ca, cb)) in a.cmds.iter().zip(&b.cmds).enumerate() {
            if ca.letter() != cb.letter() {
                return Err(MorphError {
                    index: i + 1,
                    from_cmd: Some(ca.letter()),
                    to_cmd: Some(cb.letter()),
                    flags: false,
                });
            }
        }
        return Err(MorphError {
            index: n + 1,
            from_cmd: a.cmds.get(n).map(|c| c.letter()),
            to_cmd: b.cmds.get(n).map(|c| c.letter()),
            flags: false,
        });
    }
    let lerp = |x: f64, y: f64| x + (y - x) * p;
    let lerp2 = |x: [f64; 2], y: [f64; 2]| [lerp(x[0], y[0]), lerp(x[1], y[1])];
    let mut cmds = Vec::with_capacity(a.cmds.len());
    for (i, (ca, cb)) in a.cmds.iter().zip(&b.cmds).enumerate() {
        let index = i + 1;
        match (*ca, *cb) {
            (Cmd::Move { to: t0 }, Cmd::Move { to: t1 }) => {
                cmds.push(Cmd::Move { to: lerp2(t0, t1) })
            }
            (Cmd::Line { to: t0 }, Cmd::Line { to: t1 }) => {
                cmds.push(Cmd::Line { to: lerp2(t0, t1) })
            }
            (
                Cmd::Cubic {
                    c1: a1,
                    c2: a2,
                    to: t0,
                },
                Cmd::Cubic {
                    c1: b1,
                    c2: b2,
                    to: t1,
                },
            ) => cmds.push(Cmd::Cubic {
                c1: lerp2(a1, b1),
                c2: lerp2(a2, b2),
                to: lerp2(t0, t1),
            }),
            (Cmd::Quad { c: c0, to: t0 }, Cmd::Quad { c: c1, to: t1 }) => cmds.push(Cmd::Quad {
                c: lerp2(c0, c1),
                to: lerp2(t0, t1),
            }),
            (
                Cmd::Arc {
                    rx: x0,
                    ry: y0,
                    rot: r0,
                    large: l0,
                    sweep: s0,
                    to: t0,
                },
                Cmd::Arc {
                    rx: x1,
                    ry: y1,
                    rot: r1,
                    large: l1,
                    sweep: s1,
                    to: t1,
                },
            ) => {
                if l0 != l1 || s0 != s1 {
                    return Err(MorphError {
                        index,
                        from_cmd: Some('A'),
                        to_cmd: Some('A'),
                        flags: true,
                    });
                }
                cmds.push(Cmd::Arc {
                    rx: lerp(x0, x1),
                    ry: lerp(y0, y1),
                    rot: lerp(r0, r1),
                    large: l0,
                    sweep: s0,
                    to: lerp2(t0, t1),
                });
            }
            (Cmd::Close { to: t0 }, Cmd::Close { to: t1 }) => {
                cmds.push(Cmd::Close { to: lerp2(t0, t1) })
            }
            _ => {
                return Err(MorphError {
                    index,
                    from_cmd: Some(ca.letter()),
                    to_cmd: Some(cb.letter()),
                    flags: false,
                });
            }
        }
    }
    Ok(Path { cmds })
}

/// Flattening tolerance in canvas pixels: recursive subdivision
/// stops here, so tables are exact to a fraction of a pixel and
/// identical across runs and workers.
const FLAT_TOLERANCE: f64 = 0.02;

/// Maximum subdivision depth: bounds the work on degenerate curves.
const FLAT_DEPTH: u32 = 16;

/// One flattened path: polyline points plus cumulative lengths, in
/// canvas pixels. Lookups walk true curve length, never parameter.
/// Tangents come from the primitives analytically (see [`Prim`]), so
/// headings stay exact even where the polyline chords lean.
#[derive(Clone, Debug)]
pub struct Table {
    pub points: Vec<[f64; 2]>,
    pub cumulative: Vec<f64>,
    pub total: f64,
    prims: Vec<Prim>,
    /// One tag per table segment: the primitive it flattens plus the
    /// parameter range it covers.
    tags: Vec<SegTag>,
}

/// One exact primitive of a path: a straight line or a cubic bezier
/// (quadratics arrive degree-elevated, arcs pre-converted, `Z` and
/// pen-lift `M` gaps as lines).
#[derive(Clone, Debug)]
enum Prim {
    Line {
        dir: [f64; 2],
    },
    Cubic {
        a: [f64; 2],
        c1: [f64; 2],
        c2: [f64; 2],
        b: [f64; 2],
    },
}

/// The source of one table segment.
#[derive(Clone, Copy, Debug)]
struct SegTag {
    prim: usize,
    t0: f64,
    t1: f64,
}

/// The derivative of a cubic at `t`.
fn cubic_derivative(a: [f64; 2], c1: [f64; 2], c2: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    let u = 1.0 - t;
    [
        3.0 * u * u * (c1[0] - a[0]) + 6.0 * u * t * (c2[0] - c1[0]) + 3.0 * t * t * (b[0] - c2[0]),
        3.0 * u * u * (c1[1] - a[1]) + 6.0 * u * t * (c2[1] - c1[1]) + 3.0 * t * t * (b[1] - c2[1]),
    ]
}

fn midpoint(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
}

/// Distance from `p` to the line through `a` and `b`.
fn flatness(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 {
        return ((p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2)).sqrt();
    }
    ((p[0] - a[0]) * dy - (p[1] - a[1]) * dx).abs() / len
}

/// Subdivide one cubic from `a`, emitting the polyline tail with
/// the parameter range each leaf covers.
fn flatten_cubic(
    out: &mut Vec<[f64; 2]>,
    tags: &mut Vec<SegTag>,
    prim: usize,
    a: [f64; 2],
    c1: [f64; 2],
    c2: [f64; 2],
    b: [f64; 2],
    t0: f64,
    t1: f64,
    depth: u32,
) {
    if depth >= FLAT_DEPTH
        || (flatness(c1, a, b) <= FLAT_TOLERANCE && flatness(c2, a, b) <= FLAT_TOLERANCE)
    {
        out.push(b);
        tags.push(SegTag { prim, t0, t1 });
        return;
    }
    let ab = midpoint(a, c1);
    let bc = midpoint(c1, c2);
    let cd = midpoint(c2, b);
    let abc = midpoint(ab, bc);
    let bcd = midpoint(bc, cd);
    let mid = midpoint(abc, bcd);
    let tm = (t0 + t1) / 2.0;
    flatten_cubic(out, tags, prim, a, ab, abc, mid, t0, tm, depth + 1);
    flatten_cubic(out, tags, prim, mid, bcd, cd, b, tm, t1, depth + 1);
}

/// Degree-elevate a quadratic to the cubic tracing it exactly.
fn elevate_quad(a: [f64; 2], c: [f64; 2], b: [f64; 2]) -> [[f64; 2]; 3] {
    [
        [(a[0] + 2.0 * c[0]) / 3.0, (a[1] + 2.0 * c[1]) / 3.0],
        [(b[0] + 2.0 * c[0]) / 3.0, (b[1] + 2.0 * c[1]) / 3.0],
        b,
    ]
}

/// Endpoint-to-center arc conversion (W3C SVG F.6.5), then one cubic
/// per quarter turn or less. Degenerate arcs (zero radii, coincident
/// endpoints) fall back to a line, never NaN.
fn arc_cubics(
    from: [f64; 2],
    rx: f64,
    ry: f64,
    rot_deg: f64,
    large: bool,
    sweep: bool,
    to: [f64; 2],
) -> Vec<[[f64; 2]; 3]> {
    let dx = (from[0] - to[0]) / 2.0;
    let dy = (from[1] - to[1]) / 2.0;
    if dx == 0.0 && dy == 0.0 {
        return Vec::new();
    }
    let phi = rot_deg.to_radians();
    let (cos_phi, sin_phi) = (phi.cos(), phi.sin());
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;
    let mut rx = rx.abs();
    let mut ry = ry.abs();
    if rx == 0.0 || ry == 0.0 {
        return vec![[from, to, to]];
    }
    let lam = x1p * x1p / (rx * rx) + y1p * y1p / (ry * ry);
    if lam > 1.0 {
        let s = lam.sqrt();
        rx *= s;
        ry *= s;
    }
    let sign = if large == sweep { -1.0 } else { 1.0 };
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let co = if den == 0.0 {
        0.0
    } else {
        (num / den).max(0.0).sqrt() * sign
    };
    let (cxp, cyp) = (co * rx * y1p / ry, -co * ry * x1p / rx);
    let (cx, cy) = (
        cos_phi * cxp - sin_phi * cyp + (from[0] + to[0]) / 2.0,
        sin_phi * cxp + cos_phi * cyp + (from[1] + to[1]) / 2.0,
    );
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let mut a = if len == 0.0 {
            0.0
        } else {
            (dot / len).clamp(-1.0, 1.0).acos()
        };
        if ux * vy - uy * vx < 0.0 {
            a = -a;
        }
        a
    };
    let theta1 = angle(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut delta = angle(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    if !sweep && delta > 0.0 {
        delta -= 2.0 * std::f64::consts::PI;
    } else if sweep && delta < 0.0 {
        delta += 2.0 * std::f64::consts::PI;
    }
    let n = (delta.abs() / (std::f64::consts::PI / 2.0)).ceil().max(1.0) as usize;
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let t1 = theta1 + delta * k as f64 / n as f64;
        let t2 = theta1 + delta * (k + 1) as f64 / n as f64;
        let alpha = 4.0 / 3.0 * ((t2 - t1) / 4.0).tan();
        let pt = |t: f64| {
            let (c, s) = (t.cos(), t.sin());
            [
                cx + rx * cos_phi * c - ry * sin_phi * s,
                cy + rx * sin_phi * c + ry * cos_phi * s,
            ]
        };
        let der = |t: f64| {
            let (c, s) = (t.cos(), t.sin());
            [
                -rx * cos_phi * s - ry * sin_phi * c,
                -rx * sin_phi * s + ry * cos_phi * c,
            ]
        };
        let (p1, p2) = (pt(t1), pt(t2));
        let (d1, d2) = (der(t1), der(t2));
        out.push([
            [p1[0] + alpha * d1[0], p1[1] + alpha * d1[1]],
            [p2[0] - alpha * d2[0], p2[1] - alpha * d2[1]],
            p2,
        ]);
    }
    out
}

/// One flattened subpath (Sprint 8 look, CAP-005/T005): the pen-down
/// run between pen lifts, as a polyline to [`FLAT_TOLERANCE`].
/// `closed` means a `Z` sealed the run back at its start; open runs
/// end at their last point. Multi-subpath shapes (holes, islands)
/// stay separate here, so fill rules can treat each contour alone —
/// unlike [`flatten`], which bridges `M` gaps for travel tables.
#[derive(Clone, Debug, PartialEq)]
pub struct Subpath {
    pub points: Vec<[f64; 2]>,
    pub closed: bool,
}

/// Flatten a path into per-subpath polylines: every curve becomes a
/// polyline to [`FLAT_TOLERANCE`], lines stay exact, `Z` seals its
/// run, and each `M` opens a fresh run. Zero-length runs never
/// appear, so every subpath holds at least two points.
pub fn subpaths(path: &Path) -> Vec<Subpath> {
    let mut out: Vec<Subpath> = Vec::new();
    let mut run: Vec<[f64; 2]> = Vec::new();
    let mut cur = [0.0, 0.0];
    // A throwaway tag sink: subdivision needs one, contours do not.
    let mut tags: Vec<SegTag> = Vec::new();
    // The pen position seeds an open run exactly like `flatten`
    // seeds its origin: a leading curve starts from the pen.
    let ensure_pen = |run: &mut Vec<[f64; 2]>, cur: [f64; 2]| {
        if run.is_empty() {
            run.push(cur);
        }
    };
    for cmd in &path.cmds {
        match *cmd {
            Cmd::Move { to } => {
                if run.len() > 1 {
                    out.push(Subpath {
                        points: std::mem::take(&mut run),
                        closed: false,
                    });
                } else {
                    run.clear();
                }
                cur = to;
            }
            Cmd::Line { to } => {
                ensure_pen(&mut run, cur);
                if to != cur {
                    run.push(to);
                }
                cur = to;
            }
            Cmd::Cubic { c1, c2, to } => {
                ensure_pen(&mut run, cur);
                flatten_cubic(&mut run, &mut tags, 0, cur, c1, c2, to, 0.0, 1.0, 0);
                tags.clear();
                cur = to;
            }
            Cmd::Quad { c, to } => {
                ensure_pen(&mut run, cur);
                let [c1, c2, b] = elevate_quad(cur, c, to);
                flatten_cubic(&mut run, &mut tags, 0, cur, c1, c2, b, 0.0, 1.0, 0);
                tags.clear();
                cur = to;
            }
            Cmd::Arc {
                rx,
                ry,
                rot,
                large,
                sweep,
                to,
            } => {
                ensure_pen(&mut run, cur);
                let from = cur;
                let cubics = arc_cubics(from, rx, ry, rot, large, sweep, to);
                if cubics.is_empty() {
                    if to != cur {
                        run.push(to);
                    }
                } else {
                    let mut a = from;
                    for [c1, c2, b] in cubics {
                        flatten_cubic(&mut run, &mut tags, 0, a, c1, c2, b, 0.0, 1.0, 0);
                        tags.clear();
                        a = b;
                    }
                }
                cur = to;
            }
            Cmd::Close { to } => {
                ensure_pen(&mut run, cur);
                if to != cur {
                    run.push(to);
                }
                cur = to;
                if run.len() > 1 {
                    out.push(Subpath {
                        points: std::mem::take(&mut run),
                        closed: true,
                    });
                } else {
                    run.clear();
                }
            }
        }
    }
    if run.len() > 1 {
        out.push(Subpath {
            points: run,
            closed: false,
        });
    }
    out
}

/// Flatten a path into an arc-length table: every curve becomes a
/// polyline to [`FLAT_TOLERANCE`], lines stay exact, `Z` draws back
/// to the subpath start, and the pen travels straight across `M`
/// gaps. Zero-length lines never become segments, so every tag
/// covers a positive parameter range.
pub fn flatten(path: &Path) -> Table {
    // Pass one: expand commands to exact primitives.
    let mut prims: Vec<Prim> = Vec::new();
    let mut start: Option<[f64; 2]> = None;
    let mut cur = [0.0, 0.0];
    for cmd in &path.cmds {
        match *cmd {
            Cmd::Move { to } => {
                match start {
                    None => start = Some(to),
                    Some(_) if to != cur => prims.push(Prim::Line {
                        dir: [to[0] - cur[0], to[1] - cur[1]],
                    }),
                    Some(_) => {}
                }
                cur = to;
            }
            Cmd::Line { to } => {
                if start.is_none() {
                    start = Some(cur);
                }
                if to != cur {
                    prims.push(Prim::Line {
                        dir: [to[0] - cur[0], to[1] - cur[1]],
                    });
                }
                cur = to;
            }
            Cmd::Cubic { c1, c2, to } => {
                if start.is_none() {
                    start = Some(cur);
                }
                prims.push(Prim::Cubic {
                    a: cur,
                    c1,
                    c2,
                    b: to,
                });
                cur = to;
            }
            Cmd::Quad { c, to } => {
                if start.is_none() {
                    start = Some(cur);
                }
                let [c1, c2, b] = elevate_quad(cur, c, to);
                prims.push(Prim::Cubic { a: cur, c1, c2, b });
                cur = to;
            }
            Cmd::Arc {
                rx,
                ry,
                rot,
                large,
                sweep,
                to,
            } => {
                if start.is_none() {
                    start = Some(cur);
                }
                let from = cur;
                let cubics = arc_cubics(from, rx, ry, rot, large, sweep, to);
                if cubics.is_empty() {
                    if to != cur {
                        prims.push(Prim::Line {
                            dir: [to[0] - cur[0], to[1] - cur[1]],
                        });
                    }
                } else {
                    let mut a = from;
                    for [c1, c2, b] in cubics {
                        prims.push(Prim::Cubic { a, c1, c2, b });
                        a = b;
                    }
                }
                cur = to;
            }
            Cmd::Close { to } => {
                if start.is_none() {
                    start = Some(cur);
                }
                if to != cur {
                    prims.push(Prim::Line {
                        dir: [to[0] - cur[0], to[1] - cur[1]],
                    });
                }
                cur = to;
            }
        }
    }
    let Some(origin) = start else {
        return Table {
            points: Vec::new(),
            cumulative: Vec::new(),
            total: 0.0,
            prims: Vec::new(),
            tags: Vec::new(),
        };
    };
    // Pass two: subdivide each primitive, tagging every leaf.
    let mut points = vec![origin];
    let mut tags: Vec<SegTag> = Vec::new();
    for (index, prim) in prims.iter().enumerate() {
        match *prim {
            Prim::Line { dir } => {
                let last = *points.last().unwrap();
                points.push([last[0] + dir[0], last[1] + dir[1]]);
                tags.push(SegTag {
                    prim: index,
                    t0: 0.0,
                    t1: 1.0,
                });
            }
            Prim::Cubic { a, c1, c2, b } => {
                flatten_cubic(&mut points, &mut tags, index, a, c1, c2, b, 0.0, 1.0, 0);
            }
        }
    }
    let mut cumulative = Vec::with_capacity(points.len());
    let mut total = 0.0;
    for (i, p) in points.iter().enumerate() {
        if i > 0 {
            let q = points[i - 1];
            total += ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt();
        }
        cumulative.push(total);
    }
    Table {
        points,
        cumulative,
        total,
        prims,
        tags,
    }
}

/// The heading of a table segment in degrees: `atan2(dy, dx)`, so
/// `+x` is 0 and `+y` (screen down) is 90, matching clip `rotation`.
pub fn degrees(dx: f64, dy: f64) -> f64 {
    dy.atan2(dx).to_degrees()
}

/// Locate the 1-based table segment carrying distance `d`: the
/// first segment whose cumulative end reaches `d`.
fn locate(table: &Table, d: f64) -> usize {
    let mut seg = table.cumulative.partition_point(|&c| c < d);
    if seg == 0 {
        seg = 1;
    }
    seg.min(table.points.len().saturating_sub(1).max(1))
}

/// Position on a flattened path at arc-length distance `d`,
/// clamped to the ends.
pub fn position_at(table: &Table, d: f64) -> [f64; 2] {
    if table.points.is_empty() {
        return [0.0, 0.0];
    }
    if table.total <= 0.0 {
        return table.points[0];
    }
    let d = d.clamp(0.0, table.total);
    let seg = locate(table, d);
    let (a, b) = (table.points[seg - 1], table.points[seg]);
    let span = table.cumulative[seg] - table.cumulative[seg - 1];
    if span <= 0.0 {
        return b;
    }
    let t = ((d - table.cumulative[seg - 1]) / span).clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// The raw direction of one table segment (0-based) at its fraction
/// `frac`: the primitive's analytic tangent, or `None` at cusps and
/// degenerate points.
fn raw_direction(table: &Table, seg: usize, frac: f64) -> Option<[f64; 2]> {
    let tag = *table.tags.get(seg)?;
    let dir = match table.prims.get(tag.prim) {
        Some(Prim::Line { dir }) => *dir,
        Some(Prim::Cubic { a, c1, c2, b }) => {
            cubic_derivative(*a, *c1, *c2, *b, tag.t0 + (tag.t1 - tag.t0) * frac)
        }
        None => return None,
    };
    if dir[0] == 0.0 && dir[1] == 0.0 {
        return None;
    }
    Some(dir)
}

/// Sample a flattened path at arc-length distance `d`: the position
/// plus the tangent heading in degrees. Distances clamp to the ends;
/// a zero-length table sits at its first point heading 0. The heading
/// is the carrying segment's analytic tangent, so curves report
/// their true heading at every distance; corners report the incoming
/// side exactly at the joint, then the outgoing side. Cusps borrow
/// the nearest directed neighbor, else 0.
pub fn sample(table: &Table, d: f64) -> ([f64; 2], f64) {
    if table.points.is_empty() {
        return ([0.0, 0.0], 0.0);
    }
    if table.total <= 0.0 {
        return (table.points[0], 0.0);
    }
    let clamped = d.clamp(0.0, table.total);
    let p = position_at(table, clamped);
    let seg = locate(table, clamped);
    let span = table.cumulative[seg] - table.cumulative[seg - 1];
    let frac = if span <= 0.0 {
        0.0
    } else {
        ((clamped - table.cumulative[seg - 1]) / span).clamp(0.0, 1.0)
    };
    if let Some(dir) = raw_direction(table, seg - 1, frac) {
        return (p, degrees(dir[0], dir[1]));
    }
    // A cusp or a degenerate leaf borrows the nearest segment with
    // a direction, forward first, else 0.
    for k in seg..table.tags.len() {
        if let Some(dir) = raw_direction(table, k, 0.0) {
            return (p, degrees(dir[0], dir[1]));
        }
    }
    for k in (0..seg.saturating_sub(1)).rev() {
        if let Some(dir) = raw_direction(table, k, 1.0) {
            return (p, degrees(dir[0], dir[1]));
        }
    }
    (p, 0.0)
}
