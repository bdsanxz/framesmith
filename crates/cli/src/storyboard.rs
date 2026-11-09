//! Storyboard approval gate verbs (Sprint 20, T004 / CAP-004, J-02,
//! STATE-04, REC-002).
//!
//! `storyboard board` presents one board of locked stills in one page
//! format (still PNGs plus `board.html`, no video, no slideshow) and
//! persists the locked frames for resume; `storyboard approve` records
//! the maker approval; `storyboard status` reads the gate back (a
//! missing state reads as not approved). Presenting and approving
//! travel the shared Sprint 15 patch and history machinery through the
//! preview gate module, so every call returns a staged receipt naming
//! what applied plus the recorded history turn. The film itself is
//! only read: composition sources stay byte-identical until approval,
//! and while a presented board awaits approval, renders and agent
//! motion edits refuse pointing at the pending gate.

use std::path::{Path, PathBuf};

/// Usage error for the `storyboard` verb (exit 2, like every other
/// CLI usage mistake).
fn usage_error(msg: &str) -> ! {
    eprintln!("framesmith storyboard: {msg}.");
    eprintln!(
        "usage: framesmith storyboard board [--at <t,..> | --every <secs> | --from <s> --to <e>] [--out <dir>] [--title <text>] [--composition <name>] [--backend cpu|gpu|auto] [--actor <name>] <folder>"
    );
    eprintln!("       framesmith storyboard approve [--actor <name>] [--json] <folder>");
    eprintln!("       framesmith storyboard status [--json] <folder>");
    std::process::exit(2);
}

/// Parsed `storyboard board` options.
struct BoardOptions {
    at: Option<String>,
    every: Option<String>,
    from: Option<String>,
    to: Option<String>,
    out: Option<PathBuf>,
    title: Option<String>,
    composition: Option<String>,
    backend: Option<String>,
    actor: String,
    folder: PathBuf,
}

/// Parse one finite non-negative number flag for the board.
fn parse_board_time(raw: &str, flag: &str) -> f64 {
    match raw.trim().parse::<f64>() {
        Ok(v) if v.is_finite() && v >= 0.0 => v,
        _ => usage_error(&format!(
            "{flag} needs a number of film seconds at or after 0, found {raw:?}"
        )),
    }
}

/// Resolve the requested still instants to frame indexes (Sprint 20,
/// T004): exactly one of `--at` (comma times), `--every` (interval),
/// or `--from`/`--to` (range, both required together); with no time
/// flag the board holds four stills evenly spaced across the film.
/// Between-frame times snap to the nearest frame (ties round up), and
/// every refusal names the flag and the fix before any PNG is written.
fn resolve_board_frames(
    opts: &BoardOptions,
    frames_total: u64,
    fps: f64,
    duration: f64,
) -> Vec<u64> {
    if frames_total == 0 {
        usage_error("the composition has no frames; no board was presented");
    }
    let snap = |t: f64| -> u64 { ((t * fps).round() as u64).min(frames_total - 1) };
    let mut picks: Vec<u64> = Vec::new();
    match (&opts.at, &opts.every, &opts.from, &opts.to) {
        (Some(raw), None, None, None) => {
            for part in raw.split(',') {
                if part.trim().is_empty() {
                    usage_error("--at carries an empty time; pass comma seconds like --at 0,1.5");
                }
                let t = parse_board_time(part, "--at");
                if t > duration {
                    usage_error(&format!(
                        "--at time {t} runs past the composition end ({duration:.3}s)"
                    ));
                }
                picks.push(snap(t));
            }
        }
        (None, Some(raw), None, None) => {
            let step = parse_board_time(raw, "--every");
            if step <= 0.0 {
                usage_error("--every needs a positive interval in seconds");
            }
            let mut t = 0.0;
            while t <= duration {
                picks.push(snap(t.min(duration)));
                t += step;
            }
        }
        (None, None, Some(from_raw), Some(to_raw)) => {
            let from = parse_board_time(from_raw, "--from");
            let to = parse_board_time(to_raw, "--to");
            if from >= to {
                usage_error("--from must sit before --to");
            }
            if to > duration {
                usage_error(&format!(
                    "--to time {to} runs past the composition end ({duration:.3}s)"
                ));
            }
            picks.push(snap(from));
            picks.push(snap(to));
        }
        (None, None, None, None) => {
            if frames_total == 1 {
                picks.push(0);
            } else {
                for k in 0..4 {
                    picks.push((k * (frames_total - 1) / 3).min(frames_total - 1));
                }
            }
        }
        _ => usage_error(
            "pass exactly one of --at, --every, or --from with --to (or none for four evenly spaced stills)",
        ),
    }
    picks.sort_unstable();
    picks.dedup();
    picks
}

/// Canonical still file name for one locked frame (Sprint 20, T004):
/// `still-<frame>.png` at the exact frame index, so the board page
/// and the gate state name the same file for the same instant.
fn still_file_name(frame: u64) -> String {
    format!("still-{frame:04}.png")
}

/// Present one board of locked stills: render the still PNGs, write
/// the one-page `board.html`, persist the locked frames, and report
/// the staged receipt. The film is only read — nothing composes,
/// stages, or renders motion.
fn cmd_board(opts: &BoardOptions) -> i32 {
    let folder = &opts.folder;
    let backend = match opts.backend.as_deref().unwrap_or("auto") {
        "cpu" => render::render::Backend::Cpu,
        "gpu" => render::render::Backend::Gpu,
        "auto" => render::render::Backend::Auto,
        other => {
            eprintln!("framesmith storyboard: unknown --backend {other:?}: expected cpu, gpu, or auto; no board was presented.");
            return 2;
        }
    };
    if backend == render::render::Backend::Gpu && !render::gpu::gpu_available() {
        eprintln!("framesmith storyboard: no GPU renderer is available on this machine (capability probe found none); use `--backend cpu` or `--backend auto` instead. No board was presented.");
        return 1;
    }
    let mut session = match preview::PreviewSession::open(
        folder,
        preview::PreviewOptions {
            composition: opts.composition.clone(),
            backend,
            ..preview::PreviewOptions::default()
        },
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("storyboard refused: {e} No board was presented.");
            return 1;
        }
    };
    let composition = session.composition_name().to_string();
    let fps = session.fps().value();
    let duration = session.duration_secs();
    let frames_total = session.frame_count();
    if !fps.is_finite() || fps <= 0.0 {
        eprintln!("storyboard refused: composition {composition:?} carries no frame rate; no board was presented.");
        return 1;
    }
    let picks = resolve_board_frames(opts, frames_total, fps, duration);
    let out_dir = match &opts.out {
        Some(dir) => dir.clone(),
        None => folder.join("storyboard"),
    };
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!(
            "storyboard refused: the board directory {} cannot be prepared: {e}. No board was presented.",
            out_dir.display()
        );
        return 1;
    }
    let mut locked = Vec::with_capacity(picks.len());
    for frame in picks {
        let at = if fps > 0.0 { frame as f64 / fps } else { 0.0 };
        let png = match session.frame_png(frame) {
            Ok(got) => got.png,
            Err(e) => {
                eprintln!("storyboard refused: {e} No board was presented.");
                return 1;
            }
        };
        let name = still_file_name(frame);
        if let Err(e) = std::fs::write(out_dir.join(&name), &png) {
            eprintln!(
                "storyboard refused: the still {} cannot be written: {e}. No board was presented.",
                out_dir.join(&name).display()
            );
            return 1;
        }
        locked.push(preview::studio_stills::LockedFrame {
            at,
            frame,
            label: format!("{composition} at {at:.3}s"),
            still: name,
        });
    }
    let title = opts
        .title
        .clone()
        .unwrap_or_else(|| format!("Storyboard — {composition}"));
    let page = preview::studio_stills::render_board_page(&title, &composition, &locked, false);
    if let Err(e) = std::fs::write(out_dir.join("board.html"), &page) {
        eprintln!(
            "storyboard refused: the board page {} cannot be written: {e}. No board was presented.",
            out_dir.join("board.html").display()
        );
        return 1;
    }
    match preview::studio_stills::present_board(folder, &opts.actor, &composition, &title, locked) {
        Ok(receipt) => {
            println!(
                "storyboard: board presented: {} locked still(s) from composition {composition:?}",
                receipt.frames.len()
            );
            println!(
                "storyboard: stills and board.html written to {}",
                out_dir.display()
            );
            println!(
                "storyboard: locked frames persist in .framesmith/storyboard.json; the film is unchanged"
            );
            println!(
                "storyboard: staged receipt: {} (turn {} by {}); approve with `storyboard approve` (or the Studio stills toggle) before motion or render",
                preview::studio_stills::board_receipt_json(&receipt),
                receipt.turn,
                receipt.actor,
            );
            0
        }
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

/// Record the maker approval for the presented stills and report the
/// staged receipt with its history turn.
fn cmd_approve(folder: &Path, actor: &str, json: bool) -> i32 {
    match preview::studio_stills::set_approval(folder, actor, true) {
        Ok(receipt) => {
            if json {
                println!(
                    "{}",
                    preview::studio_stills::approval_receipt_json(&receipt)
                );
            } else {
                println!(
                    "storyboard: stills approved: {} locked still(s); motion and render may proceed",
                    receipt.frames
                );
                println!(
                    "storyboard: staged receipt: {} (turn {} by {})",
                    preview::studio_stills::approval_receipt_json(&receipt),
                    receipt.turn,
                    receipt.actor,
                );
            }
            0
        }
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

/// Read the gate back: whether stills read as approved, the board,
/// and the locked stills. A missing state reads as not approved.
fn cmd_status(folder: &Path, json: bool) -> i32 {
    if json {
        println!("{}", preview::studio_stills::stills_status_json(folder));
        return 0;
    }
    let load = preview::studio_stills::load_storyboard(folder);
    match load.state {
        Some(state) => {
            let position = if state.approved {
                "approved"
            } else {
                "awaiting maker approval"
            };
            println!(
                "storyboard: board {:?} (composition {:?}): {} locked still(s), {position}",
                state.title,
                state.composition,
                state.board.len()
            );
            for frame in &state.board {
                println!(
                    "storyboard: - {} at {:.3}s (frame {}): {}",
                    frame.label, frame.at, frame.frame, frame.still
                );
            }
            if let Some(notice) = load.notice {
                println!("storyboard: note: {notice}");
            }
        }
        None => {
            let notice = load.notice.unwrap_or_else(|| {
                "no storyboard board is presented; the gate reads as not approved".to_string()
            });
            println!("storyboard: not approved ({notice})");
        }
    }
    0
}

/// Run the `storyboard` verb (`board`, `approve`, `status`).
pub fn cmd_storyboard(args: &[String]) -> i32 {
    // The verb rides first, like every other multi-verb command
    // (`timeline move`, `history list`, `skills list`); positionals are
    // validated by each verb parser below.
    let verb: Option<String> = match args.first() {
        Some(first) if !first.starts_with('-') => Some(first.clone()),
        _ => None,
    };
    match verb.as_deref() {
        Some("board") => {
            let mut opts = BoardOptions {
                at: None,
                every: None,
                from: None,
                to: None,
                out: None,
                title: None,
                composition: None,
                backend: None,
                actor: "maker".to_string(),
                folder: PathBuf::new(),
            };
            let mut folder: Option<PathBuf> = None;
            let mut i = 0;
            // Skip the verb wherever scanning starts (flags may precede
            // it on the command line).
            let mut rest: Vec<String> = args.to_vec();
            if let Some(pos) = rest.iter().position(|a| a == "board") {
                rest.remove(pos);
            }
            while i < rest.len() {
                match rest[i].as_str() {
                    "--at" => {
                        i += 1;
                        if i >= rest.len() {
                            usage_error(
                                "--at needs a value (comma film seconds, for example --at 0,1.5)",
                            );
                        }
                        if opts.at.is_some() {
                            usage_error("duplicate --at flag");
                        }
                        opts.at = Some(rest[i].clone());
                    }
                    "--every" => {
                        i += 1;
                        if i >= rest.len() {
                            usage_error("--every needs a value (interval seconds)");
                        }
                        if opts.every.is_some() {
                            usage_error("duplicate --every flag");
                        }
                        opts.every = Some(rest[i].clone());
                    }
                    "--from" => {
                        i += 1;
                        if i >= rest.len() {
                            usage_error("--from needs a value (range start seconds)");
                        }
                        if opts.from.is_some() {
                            usage_error("duplicate --from flag");
                        }
                        opts.from = Some(rest[i].clone());
                    }
                    "--to" => {
                        i += 1;
                        if i >= rest.len() {
                            usage_error("--to needs a value (range end seconds)");
                        }
                        if opts.to.is_some() {
                            usage_error("duplicate --to flag");
                        }
                        opts.to = Some(rest[i].clone());
                    }
                    "--out" => {
                        i += 1;
                        if i >= rest.len() {
                            usage_error("--out needs a value (a directory)");
                        }
                        if opts.out.is_some() {
                            usage_error("duplicate --out flag");
                        }
                        opts.out = Some(PathBuf::from(&rest[i]));
                    }
                    "--title" => {
                        i += 1;
                        if i >= rest.len() {
                            usage_error("--title needs a value (board title text)");
                        }
                        if opts.title.is_some() {
                            usage_error("duplicate --title flag");
                        }
                        opts.title = Some(rest[i].clone());
                    }
                    "--composition" => {
                        i += 1;
                        if i >= rest.len() {
                            usage_error("--composition needs a value (a composition name)");
                        }
                        if opts.composition.is_some() {
                            usage_error("duplicate --composition flag");
                        }
                        opts.composition = Some(rest[i].clone());
                    }
                    "--backend" => {
                        i += 1;
                        if i >= rest.len() {
                            usage_error("--backend needs a value (cpu, gpu, or auto)");
                        }
                        if opts.backend.is_some() {
                            usage_error("duplicate --backend flag");
                        }
                        opts.backend = Some(rest[i].clone());
                    }
                    "--actor" => {
                        i += 1;
                        if i >= rest.len() {
                            usage_error("--actor needs a value (an actor name)");
                        }
                        opts.actor = rest[i].clone();
                    }
                    other if other.starts_with('-') => {
                        usage_error(&format!("unknown flag {other:?}"));
                    }
                    other => {
                        if folder.is_some() {
                            usage_error(&format!(
                                "expected exactly one <folder>, found extra {other:?}"
                            ));
                        }
                        folder = Some(PathBuf::from(other));
                    }
                }
                i += 1;
            }
            match folder {
                Some(f) => {
                    opts.folder = f;
                    if opts.actor.trim().is_empty() {
                        usage_error("--actor needs a non-empty actor name");
                    }
                    cmd_board(&opts)
                }
                None => usage_error("expected exactly one <folder>"),
            }
        }
        Some("approve") => {
            let mut actor = "maker".to_string();
            let mut json = false;
            let mut folder: Option<PathBuf> = None;
            let mut rest: Vec<String> = args.to_vec();
            if let Some(pos) = rest.iter().position(|a| a == "approve") {
                rest.remove(pos);
            }
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--actor" => {
                        i += 1;
                        if i >= rest.len() {
                            usage_error("--actor needs a value (an actor name)");
                        }
                        actor = rest[i].clone();
                    }
                    "--json" => {
                        if json {
                            usage_error("duplicate --json flag");
                        }
                        json = true;
                    }
                    other if other.starts_with('-') => {
                        usage_error(&format!("unknown flag {other:?}"));
                    }
                    other => {
                        if folder.is_some() {
                            usage_error(&format!(
                                "expected exactly one <folder>, found extra {other:?}"
                            ));
                        }
                        folder = Some(PathBuf::from(other));
                    }
                }
                i += 1;
            }
            match folder {
                Some(f) => {
                    if actor.trim().is_empty() {
                        usage_error("--actor needs a non-empty actor name");
                    }
                    cmd_approve(&f, &actor, json)
                }
                None => usage_error("expected exactly one <folder>"),
            }
        }
        Some("status") => {
            let mut json = false;
            let mut folder: Option<PathBuf> = None;
            let mut rest: Vec<String> = args.to_vec();
            if let Some(pos) = rest.iter().position(|a| a == "status") {
                rest.remove(pos);
            }
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "--json" => {
                        if json {
                            usage_error("duplicate --json flag");
                        }
                        json = true;
                    }
                    other if other.starts_with('-') => {
                        usage_error(&format!("unknown flag {other:?}"));
                    }
                    other => {
                        if folder.is_some() {
                            usage_error(&format!(
                                "expected exactly one <folder>, found extra {other:?}"
                            ));
                        }
                        folder = Some(PathBuf::from(other));
                    }
                }
                i += 1;
            }
            match folder {
                Some(f) => cmd_status(&f, json),
                None => usage_error("expected exactly one <folder>"),
            }
        }
        Some(other) => usage_error(&format!(
            "unknown storyboard verb {other:?}: expected board, approve, or status"
        )),
        None => usage_error("expected a verb (board, approve, or status) plus <folder>"),
    }
}
