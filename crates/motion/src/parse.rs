//! Reads the text of a motion block into tweens and findings.

use crate::json::{self, Json, Member, Node};
use crate::{Ease, Kind, Prop, Tween, Values};
use timebase::Time;

/// A problem found in a motion block. `line` counts from 1 within the block's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub code: &'static str,
    pub line: u32,
    pub description: String,
    pub remedy: String,
}

/// What a motion block holds once read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Block {
    /// The tweens that are written correctly, each with its line.
    pub tweens: Vec<(Tween, u32)>,
    /// The composition the block names, when it names one.
    pub composition: Option<(String, u32)>,
    /// How many tweens the block lists, including faulty ones.
    pub written: usize,
    pub findings: Vec<Finding>,
}

const UNSUPPORTED_OPTIONS: [&str; 6] = ["repeat", "repeat-delay", "alternate", "stagger", "delay", "label"];
const UNSUPPORTED_KINDS: [&str; 2] = ["keyframes", "path"];
const UNSUPPORTED_BLOCK_FIELDS: [&str; 2] = ["labels", "defaults"];
const UNSUPPORTED_PROPS: [&str; 15] = [
    "scale-x",
    "scale-y",
    "skew-x",
    "skew-y",
    "rotation-x",
    "rotation-y",
    "z",
    "width",
    "height",
    "color",
    "background-color",
    "border-radius",
    "blur",
    "x-percent",
    "y-percent",
];
const UNSUPPORTED_EASE_FAMILIES: [&str; 7] = ["quint", "sine", "expo", "circ", "back", "elastic", "bounce"];
const UNSUPPORTED_EASE_OTHERS: [&str; 3] = ["steps", "spring", "cubic-bezier"];

const SUPPORTED_EASES: &str =
    "linear, in-quad, out-quad, in-out-quad, in-cubic, out-cubic, in-out-cubic, in-quart, out-quart, in-out-quart";

fn is_unsupported_ease(name: &str) -> bool {
    let family = name
        .strip_prefix("in-out-")
        .or_else(|| name.strip_prefix("in-"))
        .or_else(|| name.strip_prefix("out-"));
    if let Some(family) = family {
        if UNSUPPORTED_EASE_FAMILIES.contains(&family) {
            return true;
        }
    }
    UNSUPPORTED_EASE_OTHERS
        .iter()
        .any(|other| name == *other || name.starts_with(&format!("{other}(")))
}

struct Sink {
    findings: Vec<Finding>,
}

impl Sink {
    fn add(&mut self, code: &'static str, line: u32, description: String, remedy: &str) {
        self.findings.push(Finding {
            code,
            line,
            description,
            remedy: remedy.to_string(),
        });
    }
}

/// Reads one motion block.
pub fn parse_block(text: &str) -> Block {
    let mut block = Block::default();
    let mut sink = Sink { findings: Vec::new() };
    let root = match json::parse(text) {
        Ok(root) => root,
        Err(err) => {
            sink.add(
                "FS-E040",
                err.line,
                format!("The motion block is not valid JSON: {}.", err.message),
                "Correct the JSON at this line; field names and text need double quotes, and no comma may follow the last item.",
            );
            block.findings = sink.findings;
            return block;
        }
    };
    let tweens: Option<&Node> = match &root.value {
        Json::Array(_) => Some(&root),
        Json::Object(members) => read_block_object(members, root.line, &mut block, &mut sink),
        other => {
            sink.add(
                "FS-E041",
                root.line,
                format!("The motion block is {}, not an object with a \"tweens\" list.", other.kind_name()),
                "Write the block as { \"tweens\": [ … ] }.",
            );
            None
        }
    };
    if let Some(list) = tweens {
        match &list.value {
            Json::Array(items) => {
                block.written = items.len();
                for (index, item) in items.iter().enumerate() {
                    if let Some(tween) = read_tween(item, index + 1, &mut sink) {
                        block.tweens.push((tween, item.line));
                    }
                }
            }
            other => sink.add(
                "FS-E041",
                list.line,
                format!("\"tweens\" is {}, not a list.", other.kind_name()),
                "Write \"tweens\" as a list of tween objects: \"tweens\": [ { … } ].",
            ),
        }
    }
    block.findings = sink.findings;
    block
}

fn read_block_object<'a>(members: &'a [Member], line: u32, block: &mut Block, sink: &mut Sink) -> Option<&'a Node> {
    let mut tweens = None;
    for member in members {
        match member.key.as_str() {
            "tweens" => tweens = Some(&member.value),
            "composition" => match &member.value.value {
                Json::String(name) => block.composition = Some((name.clone(), member.line)),
                other => sink.add(
                    "FS-E041",
                    member.line,
                    format!("\"composition\" is {}, not a name.", other.kind_name()),
                    "Write the composition's name in double quotes, or remove the field.",
                ),
            },
            key if UNSUPPORTED_BLOCK_FIELDS.contains(&key) => sink.add(
                "FS-U013",
                member.line,
                format!("The motion block field \"{key}\" is not supported in this version."),
                "Remove the field and write each tween's time and easing on the tween itself.",
            ),
            key => sink.add(
                "FS-X002",
                member.line,
                format!("The motion block has an unknown field \"{key}\"."),
                "A motion block has only \"composition\" and \"tweens\". Remove the field or correct its name.",
            ),
        }
    }
    if tweens.is_none() {
        sink.add(
            "FS-E041",
            line,
            "The motion block has no \"tweens\" list.".to_string(),
            "Add \"tweens\": [ … ] with at least one tween, or remove the block.",
        );
    }
    tweens
}

fn tween_name(number: usize, target: Option<&str>) -> String {
    match target {
        Some(target) => format!("Tween {number} (target \"{target}\")"),
        None => format!("Tween {number}"),
    }
}

fn read_tween(node: &Node, number: usize, sink: &mut Sink) -> Option<Tween> {
    let Json::Object(members) = &node.value else {
        sink.add(
            "FS-E041",
            node.line,
            format!("Tween {number} is {}, not an object.", node.value.kind_name()),
            "Write each tween as an object: { \"target\": …, \"to\": { … }, \"at\": …, \"duration\": … }.",
        );
        return None;
    };
    let before = sink.findings.len();
    let field = |key: &str| members.iter().rev().find(|member| member.key == key);
    let target = match field("target").map(|member| (&member.value.value, member.line)) {
        Some((Json::String(text), _)) if !text.trim().is_empty() => Some(text.trim().to_string()),
        Some((Json::String(_), line)) => {
            sink.add(
                "FS-E042",
                line,
                format!("Tween {number} has an empty \"target\"."),
                "Write a CSS selector that matches the element to animate, for example \"#title\".",
            );
            None
        }
        Some((other, line)) => {
            sink.add(
                "FS-E041",
                line,
                format!("Tween {number} has a \"target\" that is {}, not a selector.", other.kind_name()),
                "Write the target as a CSS selector in double quotes, for example \"#title\".",
            );
            None
        }
        None => {
            sink.add(
                "FS-E042",
                node.line,
                format!("Tween {number} has no \"target\"."),
                "Add \"target\" with a CSS selector that matches the element to animate, for example \"#title\".",
            );
            None
        }
    };
    let name = tween_name(number, target.as_deref());

    for member in members {
        match member.key.as_str() {
            "target" | "to" | "from" | "set" | "at" | "duration" | "ease" | "kind" => {}
            key if UNSUPPORTED_OPTIONS.contains(&key) => sink.add(
                "FS-U013",
                member.line,
                format!("{name} uses the option \"{key}\", which is not supported in this version."),
                "Remove the option. Write one tween for each movement instead.",
            ),
            key if UNSUPPORTED_KINDS.contains(&key) => sink.add(
                "FS-U010",
                member.line,
                format!("{name} is a \"{key}\" tween, which is not supported in this version."),
                "Use to, from, from-to or set tweens instead.",
            ),
            key => sink.add(
                "FS-X002",
                member.line,
                format!("{name} has an unknown field \"{key}\"."),
                "A tween has \"target\", one of \"to\", \"from\", \"set\", then \"at\", \"duration\", \"ease\" and \"kind\". Remove the field or correct its name.",
            ),
        }
    }

    let to = field("to").map(|member| read_values(member, &name, sink));
    let from = field("from").map(|member| read_values(member, &name, sink));
    let set = field("set").map(|member| read_values(member, &name, sink));
    let kind = match (&to, &from, &set) {
        (None, None, None) => {
            sink.add(
                "FS-E042",
                node.line,
                format!("{name} has no values: none of \"to\", \"from\" or \"set\" is written."),
                "Add the values to animate, for example \"to\": { \"x\": 300 }.",
            );
            None
        }
        (None, None, Some(_)) => Some(Kind::Set),
        (_, _, Some(_)) => {
            sink.add(
                "FS-E047",
                node.line,
                format!("{name} mixes \"set\" with \"to\" or \"from\"."),
                "Keep \"set\" alone, or use \"to\" and \"from\" without \"set\".",
            );
            None
        }
        (Some(_), Some(_), None) => Some(Kind::FromTo),
        (Some(_), None, None) => Some(Kind::To),
        (None, Some(_), None) => Some(Kind::From),
    };
    if let (Some(Kind::FromTo), Some(Some(to)), Some(Some(from))) = (kind, &to, &from) {
        let same = to.iter().zip(from.iter()).all(|(a, b)| a.is_some() == b.is_some());
        if !same {
            sink.add(
                "FS-E047",
                node.line,
                format!("{name} lists different properties in \"from\" and \"to\"."),
                "List the same properties in both, each with its starting and its end value.",
            );
        }
    }

    if let Some(member) = field("kind") {
        match &member.value.value {
            Json::String(written) => match Kind::from_name(written) {
                Some(written_kind) => {
                    if kind.is_some_and(|kind| kind != written_kind) {
                        sink.add(
                            "FS-E047",
                            member.line,
                            format!(
                                "{name} says its kind is \"{written}\", but its value fields make it a {} tween.",
                                kind.map(Kind::name).unwrap_or_default()
                            ),
                            "Make \"kind\" agree with the value fields, or remove \"kind\".",
                        );
                    }
                }
                None if UNSUPPORTED_KINDS.contains(&written.as_str()) => sink.add(
                    "FS-U010",
                    member.line,
                    format!("{name} is a \"{written}\" tween, which is not supported in this version."),
                    "Use to, from, from-to or set tweens instead.",
                ),
                None => sink.add(
                    "FS-X003",
                    member.line,
                    format!("{name} has an unknown kind \"{written}\"."),
                    "The kinds are \"to\", \"from\", \"from-to\" and \"set\". Correct the name or remove \"kind\".",
                ),
            },
            other => sink.add(
                "FS-E041",
                member.line,
                format!("{name} has a \"kind\" that is {}, not a name.", other.kind_name()),
                "Write \"kind\" as \"to\", \"from\", \"from-to\" or \"set\", or remove it.",
            ),
        }
    }

    let at = match field("at") {
        Some(member) => read_time(member, &name, "start time \"at\"", sink),
        None => {
            sink.add(
                "FS-E042",
                node.line,
                format!("{name} has no start time \"at\"."),
                "Add \"at\" with the time in seconds at which the tween starts, for example \"at\": 1.",
            );
            None
        }
    };
    let duration = match (field("duration"), set.is_some()) {
        (Some(member), true) => {
            sink.add(
                "FS-E047",
                member.line,
                format!("{name} is a set tween and has a \"duration\"."),
                "Remove \"duration\": a set tween applies its values at one instant.",
            );
            None
        }
        (Some(member), false) => read_time(member, &name, "\"duration\"", sink),
        (None, true) => Some(Time::ZERO),
        (None, false) => {
            sink.add(
                "FS-E042",
                node.line,
                format!("{name} has no \"duration\"."),
                "Add \"duration\" with the tween's length in seconds, for example \"duration\": 0.5.",
            );
            None
        }
    };
    let ease = match field("ease").map(|member| (&member.value.value, member.line)) {
        None => Some(Ease::Linear),
        Some((Json::String(written), line)) => match Ease::from_name(written) {
            Some(ease) => Some(ease),
            None if is_unsupported_ease(written) => {
                sink.add(
                    "FS-U011",
                    line,
                    format!("{name} uses the easing \"{written}\", which is not supported in this version."),
                    &format!("Use one of: {SUPPORTED_EASES}."),
                );
                None
            }
            None => {
                sink.add(
                    "FS-X005",
                    line,
                    format!("{name} has an unknown easing \"{written}\"."),
                    &format!("The easings are: {SUPPORTED_EASES}. Correct the name."),
                );
                None
            }
        },
        Some((other, line)) => {
            sink.add(
                "FS-E045",
                line,
                format!("{name} has an \"ease\" that is {}, not an easing name.", other.kind_name()),
                &format!("Write one of: {SUPPORTED_EASES}."),
            );
            None
        }
    };

    if sink.findings.len() > before {
        return None;
    }
    let values = |slot: Option<Option<Values>>| slot.flatten().unwrap_or([None; 5]);
    let kind = kind?;
    let (from, to) = match kind {
        Kind::Set => ([None; 5], values(set)),
        _ => (values(from), values(to)),
    };
    Some(Tween {
        target: target?,
        kind,
        from,
        to,
        at: at?,
        duration: duration?,
        ease: ease?,
    })
}

fn read_time(member: &Member, name: &str, what: &str, sink: &mut Sink) -> Option<Time> {
    let Json::Number(text) = &member.value.value else {
        sink.add(
            "FS-E045",
            member.line,
            format!(
                "{name} has a {what} that is {}, not a number of seconds.",
                member.value.value.kind_name()
            ),
            "Write the time as a plain number of seconds, for example 1.5.",
        );
        return None;
    };
    let Some(time) = Time::parse(text) else {
        sink.add(
            "FS-E045",
            member.line,
            format!("{name} has a {what} of {text}, which is not a plain decimal number of seconds."),
            "Write the time as a plain number with at most nine decimal places, for example 1.5.",
        );
        return None;
    };
    if time.is_negative() {
        sink.add(
            "FS-E043",
            member.line,
            format!("{name} has a negative {what} ({text})."),
            "Write a time of 0 or more.",
        );
        return None;
    }
    Some(time)
}

fn read_values(member: &Member, name: &str, sink: &mut Sink) -> Option<Values> {
    let key = &member.key;
    let Json::Object(fields) = &member.value.value else {
        sink.add(
            "FS-E041",
            member.line,
            format!(
                "{name} has a \"{key}\" that is {}, not an object of values.",
                member.value.value.kind_name()
            ),
            "Write the values as an object, for example { \"x\": 300, \"opacity\": 1 }.",
        );
        return None;
    };
    if fields.is_empty() {
        sink.add(
            "FS-E042",
            member.line,
            format!("{name} has no values in \"{key}\"."),
            "Add at least one of \"x\", \"y\", \"scale\", \"rotation\", \"opacity\".",
        );
        return None;
    }
    let mut values: Values = [None; 5];
    let mut ok = true;
    for field in fields {
        let Some(prop) = Prop::from_name(&field.key) else {
            ok = false;
            if UNSUPPORTED_PROPS.contains(&field.key.as_str()) {
                sink.add(
                    "FS-U012",
                    field.line,
                    format!("{name} animates \"{}\", which is not supported in this version.", field.key),
                    "Animate only \"x\", \"y\", \"scale\", \"rotation\" and \"opacity\".",
                );
            } else {
                sink.add(
                    "FS-X004",
                    field.line,
                    format!("{name} has an unknown property \"{}\".", field.key),
                    "The properties are \"x\", \"y\", \"scale\", \"rotation\" and \"opacity\". Correct the name.",
                );
            }
            continue;
        };
        let number = match &field.value.value {
            Json::Number(text) => text.parse::<f64>().ok().filter(|n| n.is_finite()),
            _ => None,
        };
        let Some(number) = number else {
            ok = false;
            sink.add(
                "FS-E045",
                field.line,
                format!("{name} gives \"{}\" a value that is not a number.", field.key),
                "Write the value as a plain number, for example 300 or 0.5.",
            );
            continue;
        };
        if prop == Prop::Opacity && !(0.0..=1.0).contains(&number) {
            ok = false;
            sink.add(
                "FS-E045",
                field.line,
                format!("{name} gives \"opacity\" the value {number}, which is outside 0 to 1."),
                "Write an opacity from 0 (invisible) to 1 (fully visible).",
            );
            continue;
        }
        values[prop.index()] = Some(number);
    }
    ok.then_some(values)
}
