//! The count of differing pixels held to `pixelmatch` 8.0.0, run with `bun` (decision A18: a
//! comparison tool, not a dependency). Yardstick test: it starts `bun` from outside, to compare
//! (`docs/dev/gates.md`, section 5). Where `bun` is not on the search path, or `pixelmatch` is not in
//! the modules folder, the test is skipped by name and nothing was compared.
//!
//! The modules folder is the folder the variable FRAMESMITH_COMPARISON_MODULES names, or else
//! `pixelmatch/node_modules` of the comparison folder (where `blackbox::default_folder` finds it;
//! decision B8), or else that folder's own `node_modules`.

mod common;

use common::folder;
use measure::Frame;
use parity::blackbox;
use parity::difference;
use std::path::PathBuf;

fn modules() -> PathBuf {
    if let Some(folder) = std::env::var_os("FRAMESMITH_COMPARISON_MODULES") {
        return PathBuf::from(folder);
    }
    let comparison = blackbox::default_folder(&std::env::current_dir().unwrap_or_default());
    let own = comparison.join("pixelmatch").join("node_modules");
    if own.join("pixelmatch").is_dir() {
        own
    } else {
        comparison.join("node_modules")
    }
}

fn present() -> Result<(), String> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    if !measure::yardstick::missing_on(&path, &["bun"]).is_empty() {
        return Err("bun was not found in any folder of the search path".to_string());
    }
    let folder = modules();
    if !folder.join("pixelmatch").join("package.json").is_file() {
        return Err(format!("pixelmatch is not installed under {}", folder.display()));
    }
    Ok(())
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }

    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }

    fn unit(&mut self) -> f64 {
        f64::from(self.next()) / f64::from(1u32 << 31)
    }
}

fn frame(width: u32, height: u32, rgb: Vec<u8>) -> Frame {
    Frame { width, height, rgb }
}

/// Pairs of single pixels: half unrelated colours, half colours close to each other.
fn colour_pairs(random: &mut Lcg, count: usize) -> Vec<(Frame, Frame)> {
    (0..count)
        .map(|at| {
            let a: Vec<u8> = (0..3).map(|_| random.below(256) as u8).collect();
            let b: Vec<u8> = if at % 2 == 0 {
                (0..3).map(|_| random.below(256) as u8).collect()
            } else {
                a.iter()
                    .map(|&v| (i32::from(v) + random.below(81) as i32 - 40).clamp(0, 255) as u8)
                    .collect()
            };
            (frame(1, 1, a), frame(1, 1, b))
        })
        .collect()
}

#[derive(Clone, Copy)]
enum Shape {
    Disc { cx: f64, cy: f64, r: f64 },
    Line { cx: f64, cy: f64, angle: f64, r: f64 },
    Rect { x0: f64, y0: f64, x1: f64, y1: f64 },
}

/// A picture of shapes with anti-aliased edges (each pixel mixes its colour by the area covered),
/// moved a little by `(dx, dy, dr)`.
fn scene(width: u32, height: u32, shapes: &[(Shape, [u8; 3])], (dx, dy, dr): (f64, f64, f64)) -> Frame {
    let mut rgb: Vec<u8> = [20u8, 30, 50].iter().copied().cycle().take((width * height * 3) as usize).collect();
    for (shape, colour) in shapes {
        for y in 0..height {
            for x in 0..width {
                let (px, py) = (f64::from(x), f64::from(y));
                let cover = match *shape {
                    Shape::Disc { cx, cy, r } => (0.5 - ((px + 0.5 - (cx + dx)).hypot(py + 0.5 - (cy + dy)) - (r + dr))).clamp(0.0, 1.0),
                    Shape::Line { cx, cy, angle, r } => {
                        let d = ((px + 0.5 - cx - dx) * angle.sin() - (py + 0.5 - cy - dy) * angle.cos()).abs() - (r + dr);
                        (0.5 - d).clamp(0.0, 1.0)
                    }
                    Shape::Rect { x0, y0, x1, y1 } => {
                        let across = ((px + 1.0).min(x1 + dx) - px.max(x0 + dx)).clamp(0.0, 1.0);
                        let down = ((py + 1.0).min(y1 + dy) - py.max(y0 + dy)).clamp(0.0, 1.0);
                        across * down
                    }
                };
                if cover > 0.0 {
                    let at = ((y * width + x) * 3) as usize;
                    for k in 0..3 {
                        rgb[at + k] = (f64::from(rgb[at + k]) * (1.0 - cover) + f64::from(colour[k]) * cover).round() as u8;
                    }
                }
            }
        }
    }
    frame(width, height, rgb)
}

fn shape_pairs(random: &mut Lcg, count: usize) -> Vec<(Frame, Frame)> {
    let (width, height) = (96u32, 64u32);
    (0..count)
        .map(|trial| {
            let shapes: Vec<(Shape, [u8; 3])> = (0..2 + random.below(4))
                .map(|_| {
                    let colour = [random.below(256) as u8, random.below(256) as u8, random.below(256) as u8];
                    let shape = match random.below(3) {
                        0 => Shape::Disc {
                            cx: 10.0 + random.unit() * 70.0,
                            cy: 8.0 + random.unit() * 48.0,
                            r: 3.0 + random.unit() * 14.0,
                        },
                        1 => Shape::Line {
                            cx: 10.0 + random.unit() * 70.0,
                            cy: 8.0 + random.unit() * 48.0,
                            angle: random.unit() * std::f64::consts::PI,
                            r: 0.5 + random.unit() * 3.0,
                        },
                        _ => {
                            let (x0, y0) = (random.unit() * 60.0, random.unit() * 40.0);
                            Shape::Rect {
                                x0,
                                y0,
                                x1: x0 + 5.0 + random.unit() * 30.0,
                                y1: y0 + 5.0 + random.unit() * 20.0,
                            }
                        }
                    };
                    (shape, colour)
                })
                .collect();
            let jitter = match trial % 4 {
                0 => (0.3, 0.2, 0.4),
                1 => (0.5, 0.0, 0.0),
                2 => (0.0, 0.0, 1.2),
                _ => (0.15, 0.35, -0.3),
            };
            (
                scene(width, height, &shapes, (0.0, 0.0, 0.0)),
                scene(width, height, &shapes, jitter),
            )
        })
        .collect()
}

/// Pictures of blurred noise, the second the first with some pixels moved by a random amount.
fn noise_pairs(random: &mut Lcg, count: usize) -> Vec<(Frame, Frame)> {
    let (width, height) = (64u32, 48u32);
    (0..count)
        .map(|trial| {
            let base: Vec<u8> = (0..width * height * 3).map(|_| random.below(256) as u8).collect();
            let strength = 4 + (trial as u32 % 6) * 12;
            let other: Vec<u8> = base
                .iter()
                .map(|&v| {
                    if random.below(3) == 0 {
                        (i32::from(v) + random.below(2 * strength + 1) as i32 - strength as i32).clamp(0, 255) as u8
                    } else {
                        v
                    }
                })
                .collect();
            (frame(width, height, base), frame(width, height, other))
        })
        .collect()
}

fn rgba(frame: &Frame) -> Vec<u8> {
    frame.rgb.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect()
}

const SCRIPT: &str = r#"
import { readFileSync } from "node:fs";
const [modules, file] = process.argv.slice(2);
const { default: pixelmatch } = await import(modules + "/pixelmatch/index.js");
const bytes = readFileSync(file);
const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
let at = 0;
const count = view.getUint32(at, true); at += 4;
const counts = [];
for (let i = 0; i < count; i++) {
  const w = view.getUint32(at, true), h = view.getUint32(at + 4, true); at += 8;
  const size = w * h * 4;
  const a = new Uint8Array(bytes.buffer, bytes.byteOffset + at, size); at += size;
  const b = new Uint8Array(bytes.buffer, bytes.byteOffset + at, size); at += size;
  counts.push(pixelmatch(a, b, null, w, h, { threshold: 0.1 }));
}
console.log(counts.join("\n"));
"#;

fn yardstick_the_count_of_differing_pixels_is_the_count_of_pixelmatch_on_every_pair() {
    let dir = folder("yardstick_pixelmatch_counts");
    let mut random = Lcg(20261014);
    let mut pairs = colour_pairs(&mut random, 100_000);
    let colours = pairs.len();
    pairs.extend(shape_pairs(&mut random, 40));
    pairs.extend(noise_pairs(&mut random, 40));

    let mut bytes = (pairs.len() as u32).to_le_bytes().to_vec();
    for (a, b) in &pairs {
        bytes.extend_from_slice(&a.width.to_le_bytes());
        bytes.extend_from_slice(&a.height.to_le_bytes());
        bytes.extend(rgba(a));
        bytes.extend(rgba(b));
    }
    std::fs::write(dir.join("pairs.bin"), bytes).unwrap();
    std::fs::write(dir.join("count.mjs"), SCRIPT).unwrap();
    let out = std::process::Command::new("bun")
        .arg(dir.join("count.mjs"))
        .arg(std::path::absolute(modules()).unwrap())
        .arg(dir.join("pairs.bin"))
        .output()
        .expect("bun starts");
    assert!(out.status.success(), "bun: {}", String::from_utf8_lossy(&out.stderr));
    let theirs: Vec<u64> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().parse().unwrap())
        .collect();
    assert_eq!(theirs.len(), pairs.len());

    let (mut different, mut antialiased_pairs) = (0usize, 0usize);
    for (at, ((a, b), want)) in pairs.iter().zip(&theirs).enumerate() {
        let got = difference::compare(a, b).unwrap().differing;
        let kind = if at < colours {
            "colours"
        } else if at < colours + 40 {
            "shapes"
        } else {
            "noise"
        };
        assert_eq!(got, *want, "pair {at} ({kind}): ours {got}, pixelmatch {want}");
        different += usize::from(*want > 0);
        antialiased_pairs += usize::from(at >= colours && *want == 0);
    }
    // The pairs are not all alike: some differ, and some of the pictures differ everywhere but in anti-aliasing.
    assert!(different > 1000, "{different} of the pairs differ");
    eprintln!(
        "{} pairs compared; {different} differ; {antialiased_pairs} picture pairs count zero",
        pairs.len()
    );
}

measure::yardstick_tests!(
    measure::yardstick::Needs {
        program: "pixelmatch, run with bun",
        check: present,
    };
    yardstick_the_count_of_differing_pixels_is_the_count_of_pixelmatch_on_every_pair,
);
