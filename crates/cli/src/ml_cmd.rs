//! ML runner verbs (Sprint 23): `models`, `transcribe`, `tts`,
//! `remove-background`, and `beats`.
//!
//! Every runner refuses with the file or piece named plus the cause
//! and the fix (exit 1); unparseable invocations exit 2. Successful
//! runs print the artifact paths they wrote.

use ml::MlConfig;

fn models_usage_error(msg: &str) -> ! {
    eprintln!("framesmith models: {msg}.");
    eprintln!("usage: framesmith models list [--json]");
    eprintln!("usage: framesmith models install <name> [--yes] [--json]");
    std::process::exit(2);
}

fn transcribe_usage_error(msg: &str) -> ! {
    eprintln!("framesmith transcribe: {msg}.");
    eprintln!(
        "usage: framesmith transcribe <input> [--engine auto|NAME|stub] [--engine-bin <path>] [--model <name>] [--language <lang>] [--to srt|vtt] [-o <out.json>]"
    );
    std::process::exit(2);
}

fn tts_usage_error(msg: &str) -> ! {
    eprintln!("framesmith tts: {msg}.");
    eprintln!(
        "usage: framesmith tts <text...> [--engine auto|NAME|stub] [--engine-bin <path>] [--model <name>] [--voice <voice>] [--speed <f>] [--language <lang>] [-o <out.wav>]"
    );
    std::process::exit(2);
}

fn bg_usage_error(msg: &str) -> ! {
    eprintln!("framesmith remove-background: {msg}.");
    eprintln!(
        "usage: framesmith remove-background <input.png> [--quality fast|balanced|precise] [--model <name>] [--engine auto|NAME|stub] [--engine-bin <path>] [-o <outdir>]"
    );
    std::process::exit(2);
}

fn beats_usage_error(msg: &str) -> ! {
    eprintln!("framesmith beats: {msg}.");
    eprintln!("usage: framesmith beats <input.wav> [-o <out.beats.json>]");
    std::process::exit(2);
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// `models list|install` (T001).
pub fn cmd_models(args: &[String]) -> i32 {
    if args.len() < 2 {
        models_usage_error("expected `list` or `install <name>`");
    }
    let cfg = MlConfig::from_env();
    match args[1].as_str() {
        "list" => {
            let mut json = false;
            for arg in &args[2..] {
                if *arg == "--json" {
                    if json {
                        models_usage_error("duplicate --json flag");
                    }
                    json = true;
                } else {
                    models_usage_error(&format!("unknown flag {arg:?}"));
                }
            }
            if json {
                let mut out = String::from(
                    "{\"tool\":\"framesmith\",\"command\":\"models-list\",\"models\":[",
                );
                for (i, m) in ml::KNOWN_MODELS.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    let state = match ml::model_status(&cfg, m) {
                        ml::ModelState::Ready => "ready",
                        ml::ModelState::Missing => "missing",
                    };
                    out.push_str(&format!(
                        "{{\"name\":\"{}\",\"kind\":\"{}\",\"engine\":\"{}\",\"size\":{},\"state\":\"{state}\",\"blurb\":\"{}\"}}",
                        json_escape(m.name),
                        json_escape(m.kind),
                        json_escape(m.engine),
                        m.size_bytes,
                        json_escape(m.blurb),
                    ));
                }
                out.push_str("]}");
                println!("{out}");
            } else {
                for line in ml::models::list_lines(&cfg) {
                    println!("{line}");
                }
            }
            0
        }
        "install" => {
            let mut name: Option<String> = None;
            let mut yes = false;
            let mut json = false;
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--yes" => {
                        if yes {
                            models_usage_error("duplicate --yes flag");
                        }
                        yes = true;
                    }
                    "--json" => {
                        if json {
                            models_usage_error("duplicate --json flag");
                        }
                        json = true;
                    }
                    other if other.starts_with('-') => {
                        models_usage_error(&format!("unknown flag {other:?}"));
                    }
                    other => {
                        if name.is_some() {
                            models_usage_error(&format!(
                                "expected one <name>, found extra {other:?}"
                            ));
                        }
                        name = Some(other.to_string());
                    }
                }
                i += 1;
            }
            let Some(name) = name else {
                models_usage_error("expected one <name>");
            };
            match ml::install_model_with_consent(&cfg, &name, yes) {
                ml::InstallOutcome::Installed { announcements, .. } => {
                    for a in &announcements {
                        println!("{a}");
                    }
                    if json {
                        println!(
                            "{{\"tool\":\"framesmith\",\"command\":\"models-install\",\"model\":\"{}\",\"status\":\"installed\",\"exit\":0}}",
                            json_escape(&name)
                        );
                    }
                    0
                }
                ml::InstallOutcome::Refused { code, message } => {
                    if json {
                        println!(
                            "{{\"tool\":\"framesmith\",\"command\":\"models-install\",\"model\":\"{}\",\"status\":\"refused\",\"exit\":1,\"code\":\"{code}\",\"message\":\"{}\"}}",
                            json_escape(&name),
                            json_escape(&message)
                        );
                    } else {
                        eprintln!("{message}");
                    }
                    1
                }
            }
        }
        other => models_usage_error(&format!(
            "expected `list` or `install <name>`, found {other:?}"
        )),
    }
}

/// `transcribe <input>` (T002).
pub fn cmd_transcribe(args: &[String]) -> i32 {
    let mut opts = ml::transcribe::TranscribeOptions::default();
    let mut input: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--out" => {
                i += 1;
                if i >= args.len() {
                    transcribe_usage_error("-o needs a value (an output path)");
                }
                if opts.out.is_some() {
                    transcribe_usage_error("duplicate -o flag");
                }
                opts.out = Some(args[i].clone());
            }
            "--engine" => {
                i += 1;
                if i >= args.len() {
                    transcribe_usage_error(
                        "--engine needs a value (auto, an engine name, or stub)",
                    );
                }
                opts.engine = args[i].clone();
            }
            "--engine-bin" => {
                i += 1;
                if i >= args.len() {
                    transcribe_usage_error("--engine-bin needs a value (an engine binary path)");
                }
                if opts.engine_bin.is_some() {
                    transcribe_usage_error("duplicate --engine-bin flag");
                }
                opts.engine_bin = Some(args[i].clone());
            }
            "--model" => {
                i += 1;
                if i >= args.len() {
                    transcribe_usage_error("--model needs a value (a model name)");
                }
                opts.model = args[i].clone();
            }
            "--language" => {
                i += 1;
                if i >= args.len() {
                    transcribe_usage_error("--language needs a value");
                }
                opts.language = args[i].clone();
            }
            "--to" => {
                i += 1;
                if i >= args.len() {
                    transcribe_usage_error("--to needs a value (srt or vtt)");
                }
                if opts.to.is_some() {
                    transcribe_usage_error("duplicate --to flag");
                }
                opts.to = Some(args[i].clone());
            }
            other if other.starts_with('-') => {
                transcribe_usage_error(&format!("unknown flag {other:?}"));
            }
            other => {
                if input.is_some() {
                    transcribe_usage_error(&format!("expected one <input>, found extra {other:?}"));
                }
                input = Some(other.to_string());
            }
        }
        i += 1;
    }
    let Some(input) = input else {
        transcribe_usage_error("expected one <input>");
    };
    let cfg = MlConfig::from_env();
    match ml::transcribe::run_transcribe(&cfg, &input, &opts) {
        Ok((json_path, sidecar)) => {
            println!("transcribed {input:?} to {json_path:?}.");
            if let Some(side) = sidecar {
                println!("converted to {side:?}.");
            }
            0
        }
        Err((_, message)) => {
            eprintln!("{message}");
            1
        }
    }
}

/// `tts <text...>` (T003).
pub fn cmd_tts(args: &[String]) -> i32 {
    let mut opts = ml::tts::TtsOptions::default();
    let mut words: Vec<String> = Vec::new();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--out" => {
                i += 1;
                if i >= args.len() {
                    tts_usage_error("-o needs a value (an output WAV path)");
                }
                if opts.out.is_some() {
                    tts_usage_error("duplicate -o flag");
                }
                opts.out = Some(args[i].clone());
            }
            "--engine" => {
                i += 1;
                if i >= args.len() {
                    tts_usage_error("--engine needs a value (auto, an engine name, or stub)");
                }
                opts.engine = args[i].clone();
            }
            "--engine-bin" => {
                i += 1;
                if i >= args.len() {
                    tts_usage_error("--engine-bin needs a value (an engine binary path)");
                }
                if opts.engine_bin.is_some() {
                    tts_usage_error("duplicate --engine-bin flag");
                }
                opts.engine_bin = Some(args[i].clone());
            }
            "--model" => {
                i += 1;
                if i >= args.len() {
                    tts_usage_error("--model needs a value (a model name)");
                }
                opts.model = args[i].clone();
            }
            "--voice" => {
                i += 1;
                if i >= args.len() {
                    tts_usage_error("--voice needs a value");
                }
                opts.voice = args[i].clone();
            }
            "--speed" => {
                i += 1;
                if i >= args.len() {
                    tts_usage_error("--speed needs a value (0.25..4.0)");
                }
                match args[i].parse::<f64>() {
                    Ok(v) => opts.speed = v,
                    Err(_) => tts_usage_error(&format!("--speed {:?} is not a number", args[i])),
                }
            }
            "--language" => {
                i += 1;
                if i >= args.len() {
                    tts_usage_error("--language needs a value");
                }
                opts.language = args[i].clone();
            }
            other if other.starts_with('-') => {
                tts_usage_error(&format!("unknown flag {other:?}"));
            }
            other => words.push(other.to_string()),
        }
        i += 1;
    }
    if words.is_empty() {
        tts_usage_error("expected text to synthesize");
    }
    let text = words.join(" ");
    let cfg = MlConfig::from_env();
    match ml::tts::run_tts(&cfg, &text, &opts) {
        Ok(path) => {
            println!(
                "synthesized {path:?} (voice {}, speed {}).",
                opts.voice, opts.speed
            );
            0
        }
        Err((_, message)) => {
            eprintln!("{message}");
            1
        }
    }
}

/// `remove-background <input.png>` (T004).
pub fn cmd_remove_background(args: &[String]) -> i32 {
    let mut quality = "balanced".to_string();
    let mut model: Option<String> = None;
    let mut engine = "auto".to_string();
    let mut engine_bin: Option<String> = None;
    let mut out: Option<String> = None;
    let mut input: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--out" => {
                i += 1;
                if i >= args.len() {
                    bg_usage_error("-o needs a value (an output directory)");
                }
                if out.is_some() {
                    bg_usage_error("duplicate -o flag");
                }
                out = Some(args[i].clone());
            }
            "--quality" => {
                i += 1;
                if i >= args.len() {
                    bg_usage_error("--quality needs a value (fast, balanced, or precise)");
                }
                quality = args[i].clone();
            }
            "--model" => {
                i += 1;
                if i >= args.len() {
                    bg_usage_error("--model needs a value (a model name)");
                }
                if model.is_some() {
                    bg_usage_error("duplicate --model flag");
                }
                model = Some(args[i].clone());
            }
            "--engine" => {
                i += 1;
                if i >= args.len() {
                    bg_usage_error("--engine needs a value (auto, an engine name, or stub)");
                }
                engine = args[i].clone();
            }
            "--engine-bin" => {
                i += 1;
                if i >= args.len() {
                    bg_usage_error("--engine-bin needs a value (an engine binary path)");
                }
                if engine_bin.is_some() {
                    bg_usage_error("duplicate --engine-bin flag");
                }
                engine_bin = Some(args[i].clone());
            }
            other if other.starts_with('-') => {
                bg_usage_error(&format!("unknown flag {other:?}"));
            }
            other => {
                if input.is_some() {
                    bg_usage_error(&format!("expected one <input>, found extra {other:?}"));
                }
                input = Some(other.to_string());
            }
        }
        i += 1;
    }
    let Some(input) = input else {
        bg_usage_error("expected one <input>");
    };
    let cfg = MlConfig::from_env();
    match ml::matting::run_remove_background(
        &cfg,
        &input,
        &quality,
        model.as_deref(),
        &engine,
        engine_bin.as_deref(),
        out.as_deref(),
    ) {
        Ok((alpha, plate)) => {
            println!("subject alpha: {alpha:?}.");
            println!("background plate: {plate:?}.");
            0
        }
        Err((_, message)) => {
            eprintln!("{message}");
            1
        }
    }
}

/// `beats <input.wav>` (T004).
pub fn cmd_beats(args: &[String]) -> i32 {
    let mut out: Option<String> = None;
    let mut input: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--out" => {
                i += 1;
                if i >= args.len() {
                    beats_usage_error("-o needs a value (an output beat-file path)");
                }
                if out.is_some() {
                    beats_usage_error("duplicate -o flag");
                }
                out = Some(args[i].clone());
            }
            other if other.starts_with('-') => {
                beats_usage_error(&format!("unknown flag {other:?}"));
            }
            other => {
                if input.is_some() {
                    beats_usage_error(&format!("expected one <input>, found extra {other:?}"));
                }
                input = Some(other.to_string());
            }
        }
        i += 1;
    }
    let Some(input) = input else {
        beats_usage_error("expected one <input>");
    };
    let cfg = MlConfig::from_env();
    match ml::beats::run_beats(&cfg, &input, out.as_deref()) {
        Ok(path) => {
            let grid =
                ml::beats::parse_beat_file(&std::fs::read_to_string(&path).unwrap_or_default());
            match grid {
                Ok(g) => println!(
                    "beat file {path:?}: {:.1} BPM, {} beat(s), confidence {:.2}.",
                    g.bpm,
                    g.beats.len(),
                    g.confidence
                ),
                Err(_) => println!("beat file {path:?}."),
            }
            0
        }
        Err((_, message)) => {
            eprintln!("{message}");
            1
        }
    }
}
