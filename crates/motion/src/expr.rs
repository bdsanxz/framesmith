//! Pure seeded expressions (Sprint 7 motion, CAP-010).
//!
//! A numeric `tween` may carry `expr="..."` instead of `from`/`to`
//! numbers. The expression evaluates per frame from `t` (composition
//! seconds), `i` (piece or replicate index), and declared `input`
//! values only, with `random(seed)` and `random(seed,min,max)`
//! deterministic per seed. There are no side effects, no I/O, and no
//! unseeded randomness: anything beyond the grammar below is refused
//! at validation time, never guessed at render time.
//!
//! Grammar (all operators left-associative except `^`, which is
//! right-associative):
//!
//! ```text
//! expr   := term (("+" | "-") term)*
//! term   := factor (("*" | "/" | "%") factor)*
//! factor := unary ("^" factor)?
//! unary  := "-" unary | primary
//! primary := number | name | name "(" args ")" | "(" expr ")"
//! ```
//!
//! Names are `t`, `i`, declared input names, the constants `pi`,
//! `tau`, `e`, and the pure functions `sin`, `cos`, `tan`, `asin`,
//! `acos`, `atan`, `atan2`, `sqrt`, `cbrt`, `abs`, `sign`, `floor`,
//! `ceil`, `round`, `trunc`, `fract`, `min`, `max`, `clamp`, `pow`,
//! `exp`, `ln`, `log`, `log2`, `log10`, `hypot`, and `random`.
//! `random(seed)` draws uniform `[0, 1)`; `random(seed,min,max)`
//! scales to `[min, max)`. Both are constant per seed (a pure
//! function of the seed alone, via SplitMix64), so identical seeds
//! render identical motion on every worker and every re-render; vary
//! the seed with `i` for per-piece scatters.

/// One parsed expression, ready to evaluate at any instant.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    root: Node,
}

/// Why an expression cannot run: always authored text, never a guess.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExprError {
    /// Byte offset into the source where the problem sits.
    pub offset: usize,
    /// What is wrong, in maker vocabulary.
    pub message: String,
}

impl ExprError {
    fn at(offset: usize, message: impl Into<String>) -> Self {
        Self {
            offset,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ExprError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (at column {})", self.message, self.offset + 1)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Number(f64),
    Var(Var),
    Call(&'static str, Vec<Node>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Var {
    T,
    I,
    Input(u32),
}

/// Parse `source` into an [`Expr`], resolving bare names against
/// `inputs` (declared `input` names, in any order). Unknown names,
/// impure calls, wrong arities, and malformed text all fail.
pub fn parse(source: &str, inputs: &[&str]) -> Result<Expr, ExprError> {
    let mut parser = Parser {
        bytes: source.as_bytes(),
        pos: 0,
        inputs,
    };
    parser.skip_ws();
    if parser.pos >= parser.bytes.len() {
        return Err(ExprError::at(
            0,
            "expression is empty; write a value like `t*30`",
        ));
    }
    let root = parser.expr()?;
    parser.skip_ws();
    if parser.pos < parser.bytes.len() {
        return Err(ExprError::at(
            parser.pos,
            format!(
                "unexpected {:?} in expression; expressions hold numbers, `t`, `i`, declared inputs, and pure math calls",
                &source[parser.pos..parser.pos + 1]
            ),
        ));
    }
    Ok(Expr { root })
}

/// The pure function names an expression may call, for diagnostics.
pub const FUNCTIONS: [&str; 24] = [
    "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "sqrt", "cbrt", "abs", "sign", "floor",
    "ceil", "round", "trunc", "fract", "min", "max", "clamp", "pow", "exp", "ln", "log", "log2",
];
// NOTE: `log10`, `hypot`, and `random` are also accepted; they ride
// outside this list so the diagnostic line stays short. They are
// named in the unknown-name fix below.

/// Names that can never run in an expression: side effects and I/O.
/// Calling one fails as impure, not as merely unknown.
pub const IMPURE: [&str; 24] = [
    "open", "read", "write", "print", "exec", "system", "shell", "fetch", "http", "load", "save",
    "store", "delete", "exit", "abort", "rand", "srand", "noise", "time", "now", "frame", "dt",
    "eval", "do",
];

/// Reserved names no declared input may take: the clock, the index,
/// the constants, and every pure function name.
pub fn reserved(name: &str) -> bool {
    matches!(
        name,
        "t" | "i" | "pi" | "tau" | "e" | "random" | "hypot" | "log10"
    ) || FUNCTIONS.contains(&name)
        || IMPURE.contains(&name)
}

/// Whether `name` is a usable expression identifier: ASCII letters,
/// digits, and underscores, not starting with a digit.
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    name.len() <= 64 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    inputs: &'a [&'a str],
}

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn expr(&mut self) -> Result<Node, ExprError> {
        let mut node = self.term()?;
        loop {
            self.skip_ws();
            let op = match self.bytes.get(self.pos) {
                Some(b'+') => '+',
                Some(b'-') => '-',
                _ => break,
            };
            self.pos += 1;
            let rhs = self.term()?;
            node = Node::Call(if op == '+' { "add" } else { "sub" }, vec![node, rhs]);
        }
        Ok(node)
    }

    fn term(&mut self) -> Result<Node, ExprError> {
        let mut node = self.unary()?;
        loop {
            self.skip_ws();
            let op = match self.bytes.get(self.pos) {
                Some(b'*') => "*",
                Some(b'/') => "/",
                Some(b'%') => "%",
                _ => break,
            };
            self.pos += 1;
            let rhs = self.unary()?;
            node = Node::Call(
                match op {
                    "*" => "mul",
                    "/" => "div",
                    _ => "mod",
                },
                vec![node, rhs],
            );
        }
        Ok(node)
    }

    fn factor(&mut self) -> Result<Node, ExprError> {
        let base = self.primary()?;
        self.skip_ws();
        if self.bytes.get(self.pos) == Some(&b'^') {
            self.pos += 1;
            let exp = self.factor()?;
            return Ok(Node::Call("pow2", vec![base, exp]));
        }
        Ok(base)
    }

    fn unary(&mut self) -> Result<Node, ExprError> {
        self.skip_ws();
        if self.bytes.get(self.pos) == Some(&b'-') {
            self.pos += 1;
            return Ok(Node::Call("neg", vec![self.unary()?]));
        }
        if self.bytes.get(self.pos) == Some(&b'+') {
            return Err(ExprError::at(
                self.pos,
                "unexpected `+`; write the value plainly or negate with `-`",
            ));
        }
        self.factor()
    }

    fn primary(&mut self) -> Result<Node, ExprError> {
        self.skip_ws();
        match self.bytes.get(self.pos) {
            None => Err(ExprError::at(
                self.pos,
                "expression ends early; a value was expected here",
            )),
            Some(&b'(') => {
                self.pos += 1;
                let node = self.expr()?;
                self.skip_ws();
                match self.bytes.get(self.pos) {
                    Some(&b')') => {
                        self.pos += 1;
                        Ok(node)
                    }
                    _ => Err(ExprError::at(
                        self.pos,
                        "missing `)`; every opening paren closes",
                    )),
                }
            }
            Some(&(b'0'..=b'9')) | Some(&b'.') => self.number(),
            Some(&c) if c.is_ascii_alphabetic() || c == b'_' => self.name_or_call(),
            Some(_) => Err(ExprError::at(
                self.pos,
                "unexpected character; expressions hold numbers, `t`, `i`, declared inputs, and pure math calls",
            )),
        }
    }

    fn number(&mut self) -> Result<Node, ExprError> {
        let start = self.pos;
        let mut seen_digit = false;
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
            seen_digit = true;
            self.pos += 1;
        }
        if self.bytes.get(self.pos) == Some(&b'.') {
            // A `.` only joins the number when a digit follows, so
            // `t*2 .5` never swallows the dot silently.
            let after = self.pos + 1;
            if after < self.bytes.len() && self.bytes[after].is_ascii_digit() {
                self.pos += 1;
                while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                    seen_digit = true;
                    self.pos += 1;
                }
            }
        }
        if (self.bytes.get(self.pos) == Some(&b'e') || self.bytes.get(self.pos) == Some(&b'E'))
            && seen_digit
        {
            let mut end = self.pos + 1;
            if self.bytes.get(end) == Some(&b'+') || self.bytes.get(end) == Some(&b'-') {
                end += 1;
            }
            let digits = end;
            while end < self.bytes.len() && self.bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > digits {
                self.pos = end;
            }
        }
        if !seen_digit {
            return Err(ExprError::at(
                start,
                "expected a number; write digits like `30` or `0.5`",
            ));
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| ExprError::at(start, "expected a number"))?;
        match text.parse::<f64>() {
            Ok(v) => Ok(Node::Number(v)),
            Err(_) => Err(ExprError::at(
                start,
                format!("number {text:?} does not parse"),
            )),
        }
    }

    fn ident(&mut self) -> (String, usize) {
        let start = self.pos;
        while self.pos < self.bytes.len()
            && (self.bytes[self.pos].is_ascii_alphanumeric() || self.bytes[self.pos] == b'_')
        {
            self.pos += 1;
        }
        (
            String::from_utf8_lossy(&self.bytes[start..self.pos]).into_owned(),
            start,
        )
    }

    fn name_or_call(&mut self) -> Result<Node, ExprError> {
        let (name, start) = self.ident();
        self.skip_ws();
        if self.bytes.get(self.pos) == Some(&b'(') {
            self.pos += 1;
            let mut args = Vec::new();
            self.skip_ws();
            if self.bytes.get(self.pos) != Some(&b')') {
                loop {
                    args.push(self.expr()?);
                    self.skip_ws();
                    match self.bytes.get(self.pos) {
                        Some(&b',') => {
                            self.pos += 1;
                        }
                        Some(&b')') => break,
                        _ => {
                            return Err(ExprError::at(
                                self.pos,
                                "missing `,` or `)`; call arguments separate with commas",
                            ));
                        }
                    }
                }
            }
            self.pos += 1; // consume `)`
            return self.check_call(&name, start, args);
        }
        match name.as_str() {
            "t" => Ok(Node::Var(Var::T)),
            "i" => Ok(Node::Var(Var::I)),
            "pi" => Ok(Node::Number(std::f64::consts::PI)),
            "tau" => Ok(Node::Number(std::f64::consts::TAU)),
            "e" => Ok(Node::Number(std::f64::consts::E)),
            _ if FUNCTIONS.contains(&name.as_str())
                || matches!(name.as_str(), "random" | "hypot" | "log10") =>
            {
                Err(ExprError::at(
                    start,
                    format!(
                        "`{name}` is a function; call it with parentheses like `{name}(...)`"
                    ),
                ))
            }
            _ if IMPURE.contains(&name.as_str()) => Err(ExprError::at(
                start,
                format!(
                    "`{name}` is impure; expressions are pure math over `t`, `i`, and declared inputs, with no side effects or I/O"
                ),
            )),
            _ => match self.inputs.iter().position(|n| *n == name) {
                Some(k) => Ok(Node::Var(Var::Input(k as u32))),
                None => Err(ExprError::at(
                    start,
                    format!(
                        "unknown name `{name}`; use `t`, `i`, a declared `input`, `pi`/`tau`/`e`, or a pure math call"
                    ),
                )),
            },
        }
    }

    fn check_call(&mut self, name: &str, start: usize, args: Vec<Node>) -> Result<Node, ExprError> {
        if IMPURE.contains(&name) {
            return Err(ExprError::at(
                start,
                format!(
                    "`{name}` is impure; expressions are pure math over `t`, `i`, and declared inputs, with no side effects or I/O"
                ),
            ));
        }
        // `random` without a seed is refused; every other arity is
        // checked per function below.
        if name == "random" && args.is_empty() {
            return Err(ExprError::at(
                start,
                "`random` needs a seed; write `random(7)` or `random(7,min,max)` so re-renders draw identically",
            ));
        }
        let arity_ok = match name {
            "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sqrt" | "cbrt" | "abs" | "sign"
            | "floor" | "ceil" | "round" | "trunc" | "fract" | "exp" | "ln" | "log2" | "log10" => {
                args.len() == 1
            }
            "atan2" | "pow" | "hypot" | "min" | "max" | "log" => args.len() == 2,
            "clamp" => args.len() == 3,
            "random" => args.len() == 1 || args.len() == 3,
            _ => {
                return Err(ExprError::at(
                    start,
                    format!(
                        "unknown function `{name}`; pure calls are sin/cos/tan/asin/acos/atan/atan2/sqrt/cbrt/abs/sign/floor/ceil/round/trunc/fract/min/max/clamp/pow/exp/ln/log/log2/log10/hypot/random"
                    ),
                ));
            }
        };
        if !arity_ok {
            return Err(ExprError::at(
                start,
                format!(
                    "`{name}` takes {}, but {} {} given; check the argument count",
                    match name {
                        "random" => "1 or 3 arguments".to_string(),
                        _ => format!(
                            "{} argument{}",
                            expected_arity(name),
                            if expected_arity(name) == 1 { "" } else { "s" }
                        ),
                    },
                    args.len(),
                    if args.len() == 1 { "was" } else { "were" },
                ),
            ));
        }
        Ok(Node::Call(canonical(name), args))
    }
}

fn expected_arity(name: &str) -> usize {
    match name {
        "clamp" => 3,
        "atan2" | "pow" | "hypot" | "min" | "max" | "log" => 2,
        _ => 1,
    }
}

/// Canonical callee names: operators desugar to the same call table
/// the evaluator matches, so rendering never branches on syntax.
fn canonical(name: &str) -> &'static str {
    match name {
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "asin" => "asin",
        "acos" => "acos",
        "atan" => "atan",
        "atan2" => "atan2",
        "sqrt" => "sqrt",
        "cbrt" => "cbrt",
        "abs" => "abs",
        "sign" => "sign",
        "floor" => "floor",
        "ceil" => "ceil",
        "round" => "round",
        "trunc" => "trunc",
        "fract" => "fract",
        "min" => "min",
        "max" => "max",
        "clamp" => "clamp",
        "pow" => "pow",
        "exp" => "exp",
        "ln" => "ln",
        "log" => "log",
        "log2" => "log2",
        "log10" => "log10",
        "hypot" => "hypot",
        "random" => "random",
        _ => "unknown",
    }
}

impl Expr {
    /// Evaluate at composition seconds `t` and piece/replicate index
    /// `i`, with declared input values in declaration order. Pure:
    /// the same arguments always yield the same value on any worker.
    pub fn eval(&self, t: f64, i: f64, inputs: &[f64]) -> f64 {
        eval_node(&self.root, t, i, inputs)
    }
}

fn eval_node(node: &Node, t: f64, i: f64, inputs: &[f64]) -> f64 {
    match node {
        Node::Number(v) => *v,
        Node::Var(Var::T) => t,
        Node::Var(Var::I) => i,
        Node::Var(Var::Input(k)) => inputs.get(*k as usize).copied().unwrap_or(f64::NAN),
        Node::Call(op, args) => match *op {
            "add" => eval_node(&args[0], t, i, inputs) + eval_node(&args[1], t, i, inputs),
            "sub" => eval_node(&args[0], t, i, inputs) - eval_node(&args[1], t, i, inputs),
            "mul" => eval_node(&args[0], t, i, inputs) * eval_node(&args[1], t, i, inputs),
            "div" => eval_node(&args[0], t, i, inputs) / eval_node(&args[1], t, i, inputs),
            "mod" => eval_node(&args[0], t, i, inputs) % eval_node(&args[1], t, i, inputs),
            "pow2" => eval_node(&args[0], t, i, inputs).powf(eval_node(&args[1], t, i, inputs)),
            "neg" => -eval_node(&args[0], t, i, inputs),
            "sin" => eval_node(&args[0], t, i, inputs).sin(),
            "cos" => eval_node(&args[0], t, i, inputs).cos(),
            "tan" => eval_node(&args[0], t, i, inputs).tan(),
            "asin" => eval_node(&args[0], t, i, inputs).asin(),
            "acos" => eval_node(&args[0], t, i, inputs).acos(),
            "atan" => eval_node(&args[0], t, i, inputs).atan(),
            "atan2" => eval_node(&args[0], t, i, inputs).atan2(eval_node(&args[1], t, i, inputs)),
            "sqrt" => eval_node(&args[0], t, i, inputs).sqrt(),
            "cbrt" => eval_node(&args[0], t, i, inputs).cbrt(),
            "abs" => eval_node(&args[0], t, i, inputs).abs(),
            "sign" => {
                let v = eval_node(&args[0], t, i, inputs);
                if v > 0.0 {
                    1.0
                } else if v < 0.0 {
                    -1.0
                } else {
                    v
                }
            }
            "floor" => eval_node(&args[0], t, i, inputs).floor(),
            "ceil" => eval_node(&args[0], t, i, inputs).ceil(),
            "round" => eval_node(&args[0], t, i, inputs).round(),
            "trunc" => eval_node(&args[0], t, i, inputs).trunc(),
            "fract" => eval_node(&args[0], t, i, inputs).fract(),
            "min" => eval_node(&args[0], t, i, inputs).min(eval_node(&args[1], t, i, inputs)),
            "max" => eval_node(&args[0], t, i, inputs).max(eval_node(&args[1], t, i, inputs)),
            "clamp" => eval_node(&args[0], t, i, inputs).clamp(
                eval_node(&args[1], t, i, inputs),
                eval_node(&args[2], t, i, inputs),
            ),
            "pow" => eval_node(&args[0], t, i, inputs).powf(eval_node(&args[1], t, i, inputs)),
            "exp" => eval_node(&args[0], t, i, inputs).exp(),
            "ln" => eval_node(&args[0], t, i, inputs).ln(),
            "log" => eval_node(&args[0], t, i, inputs).log(eval_node(&args[1], t, i, inputs)),
            "log2" => eval_node(&args[0], t, i, inputs).log2(),
            "log10" => eval_node(&args[0], t, i, inputs).log10(),
            "hypot" => eval_node(&args[0], t, i, inputs).hypot(eval_node(&args[1], t, i, inputs)),
            "random" => {
                let seed = eval_node(&args[0], t, i, inputs);
                let u = seeded_unit(seed);
                if args.len() == 1 {
                    u
                } else {
                    let lo = eval_node(&args[1], t, i, inputs);
                    let hi = eval_node(&args[2], t, i, inputs);
                    lo + u * (hi - lo)
                }
            }
            _ => f64::NAN,
        },
    }
}

/// Deterministic unit draw from a seed: the seed floors to an
/// integer, SplitMix64 mixes it, and the top 53 bits scale to
/// `[0, 1)`. Constant per seed on every worker and every re-render.
pub fn seeded_unit(seed: f64) -> f64 {
    let z = if seed.is_finite() {
        seed.floor() as i64 as u64
    } else {
        0x9e37_79b9_7f4a_7c15
    };
    let mut x = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    ((x >> 11) as f64) / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(source: &str, t: f64, i: f64) -> f64 {
        parse(source, &[])
            .expect("test expression parses")
            .eval(t, i, &[])
    }

    #[test]
    fn arithmetic_follows_precedence() {
        // Hand-computed: 2+3*4^2 = 2+3*16 = 50; (2+3)*4 = 20.
        assert_eq!(eval("2+3*4^2", 0.0, 0.0), 50.0);
        assert_eq!(eval("(2+3)*4", 0.0, 0.0), 20.0);
        assert_eq!(eval("10-4/2", 0.0, 0.0), 8.0);
        assert_eq!(eval("7%3", 0.0, 0.0), 1.0);
        assert_eq!(eval("-2^2", 0.0, 0.0), -4.0);
        assert_eq!(eval("2^3^2", 0.0, 0.0), 512.0);
    }

    #[test]
    fn clock_and_index_drive_values() {
        // t=1.5, i=2: t*30+i*5 = 45+10 = 55.
        assert_eq!(eval("t*30+i*5", 1.5, 2.0), 55.0);
        assert_eq!(eval("t", 1.5, 2.0), 1.5);
        assert_eq!(eval("i", 1.5, 2.0), 2.0);
    }

    #[test]
    fn math_calls_match_std() {
        let t = 0.7f64;
        assert_eq!(eval("sin(t)", t, 0.0), t.sin());
        assert_eq!(eval("cos(t)", t, 0.0), t.cos());
        assert_eq!(eval("sqrt(2)", 0.0, 0.0), 2f64.sqrt());
        assert_eq!(
            eval("min(t,1)+max(i,3)", t, 5.0),
            t.min(1.0) + 5f64.max(3.0)
        );
        assert_eq!(eval("clamp(t,0,0.5)", t, 0.0), 0.5);
        assert_eq!(eval("pi", 0.0, 0.0), std::f64::consts::PI);
        assert_eq!(eval("pow(2,10)", 0.0, 0.0), 1024.0);
    }

    #[test]
    fn seeded_random_is_deterministic_per_seed() {
        // Same seed, same draw, at any t/i; distinct seeds differ
        // (checked loosely: at least one of three differs).
        let a = eval("random(7)", 0.0, 0.0);
        assert_eq!(a, eval("random(7)", 1.25, 9.0));
        assert!((0.0..1.0).contains(&a));
        let lo = eval("random(7,-5,5)", 0.0, 0.0);
        assert_eq!(lo, eval("random(7,-5,5)", 3.0, 4.0));
        assert!((-5.0..5.0).contains(&lo));
        // Per-piece scatter: the seed rides i.
        let s0 = eval("random(i,0,100)", 0.0, 0.0);
        let s1 = eval("random(i,0,100)", 0.0, 1.0);
        assert_ne!(s0, s1);
        assert!([eval("random(8)", 0.0, 0.0), eval("random(9)", 0.0, 0.0)]
            .iter()
            .any(|v| *v != a));
    }

    #[test]
    fn declared_inputs_resolve() {
        let expr = parse("amp*sin(t)+off", &["amp", "off"]).unwrap();
        assert_eq!(expr.eval(0.0, 0.0, &[2.0, 10.0]), 10.0);
    }

    #[test]
    fn refusals_name_the_problem() {
        // Unknown names, impure calls, unseeded randomness, bad
        // arities, and malformed text all fail.
        assert!(parse("x*2", &[])
            .unwrap_err()
            .message
            .contains("unknown name `x`"));
        assert!(parse("open(1)", &[])
            .unwrap_err()
            .message
            .contains("impure"));
        assert!(parse("random()", &[]).unwrap_err().message.contains("seed"));
        assert!(parse("sin(1,2)", &[]).is_err());
        assert!(parse("nope(1)", &[])
            .unwrap_err()
            .message
            .contains("unknown function"));
        assert!(parse("t*", &[]).is_err());
        assert!(parse("", &[]).is_err());
        assert!(parse("t;rm", &[]).is_err());
        // A declared input resolves; an undeclared one still fails.
        assert!(parse("amp", &["amp"]).is_ok());
        assert!(parse("amp", &[])
            .unwrap_err()
            .message
            .contains("unknown name"));
        // Bare function names name the call fix.
        assert!(parse("sin", &[])
            .unwrap_err()
            .message
            .contains("call it with parentheses"));
    }
}
