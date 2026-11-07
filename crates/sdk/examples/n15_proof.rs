//! T009 proof harness (Sprint 15): exercises the player-sdk reference
//! end to end on one staged project directory given as argv[1].
//!
//! Every gate prints `CHECK <name>: PASS …` and any failure prints
//! `CHECK <name>: FAIL -- …` then exits 1. A clean run ends with
//! `PROOF: PASS`. Driven by `tools/n15-player-sdk-proof.sh`, which
//! also performs the tamper negative (corrupt copy must exit non-zero).

use preview::{embed_snippet, LoadedFilm, Player, PlayerAttributes, PlayerState, CANONICAL_ORDER};
use sdk::history::{begin_turn, end_turn, list_turns, BeginOptions};
use sdk::{
    ClipRef, EditOp, OpenOptions, SdkSession, TimeExpr, TimelineVerb, VariableInput,
    E_SDK_GUARD_HELD,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn fail(name: &str, detail: String) -> ! {
    eprintln!("CHECK {name}: FAIL -- {detail}");
    std::process::exit(1);
}

fn check(name: &str, ok: bool, detail: String) {
    if ok {
        println!("CHECK {name}: PASS -- {detail}");
    } else {
        fail(name, detail);
    }
}

fn open(folder: &Path) -> SdkSession {
    match SdkSession::open(folder, OpenOptions::default()) {
        Ok(s) => s,
        Err(e) => fail("sdk-open", format!("refused: {e}")),
    }
}

fn main() {
    let dir: PathBuf = std::env::args()
        .nth(1)
        .unwrap_or_else(|| {
            eprintln!("usage: n15_proof <project-dir>");
            std::process::exit(2);
        })
        .into();
    println!("n15 proof harness on {}", dir.display());

    // ---- headless open ------------------------------------------------
    let session = open(&dir);
    check(
        "sdk-open-ready",
        session.is_ready()
            && session.controls_enabled()
            && session.frame_count() == 60
            && session.first_frame().is_some(),
        format!(
            "composition {:?} ready, {} frames, canvas {:?}, controls on",
            session.composition_name(),
            session.frame_count(),
            session.canvas()
        ),
    );

    // ---- embedded playback with ordered events -------------------------
    let attrs = PlayerAttributes::from_query("autoplay=0&controls=1");
    let mut player = Player::begin(attrs.clone());
    check(
        "player-begins-loading",
        player.state() == PlayerState::Loading,
        "begin reports Loading before the film arrives".to_string(),
    );
    let (w, h) = session.canvas();
    player.load(
        Duration::from_millis(120),
        Ok(LoadedFilm {
            width: w,
            height: h,
            frame_count: session.frame_count(),
            fps: session.fps(),
            duration_secs: session.duration_secs(),
            has_audio: false,
            font_try_order: vec!["Alpha".to_string(), "Beta".to_string()],
        }),
    );
    check(
        "player-loads-ready",
        player.state() == PlayerState::Ready && player.controls_enabled(),
        format!("ready with film {}x{} silent", w, h),
    );
    let names = player.event_names();
    let prefix: Vec<&str> = CANONICAL_ORDER[..5].to_vec();
    check(
        "player-events-ordered",
        names == prefix,
        format!("load fires {}", names.join(",")),
    );
    for e in player.events() {
        println!("EVENT {}: {}", e.kind.name(), e.detail);
    }
    println!("READOUT {}", player.readout());
    let snippet = embed_snippet(
        "http://127.0.0.1:9",
        session.composition_name(),
        &attrs,
        w,
        h,
        None,
    );
    check(
        "player-embed-snippet",
        snippet.contains("<iframe")
            && snippet.contains(session.composition_name())
            && snippet.contains("320"),
        format!("embed names the composition ({})", snippet.len()),
    );
    if player.play().is_err() {
        fail("player-play", "play refused on a ready film".to_string());
    }
    player.tick(Duration::from_millis(500));
    let after = player.event_names();
    // Later calls append further state/timeline/painted/playback events
    // in the same relative order: the post-load tail must itself be a
    // non-decreasing run through the canonical order.
    let order_ok = after
        .iter()
        .skip(names.len())
        .map(|n| CANONICAL_ORDER.iter().position(|c| c == n))
        .collect::<Option<Vec<_>>>()
        .is_some_and(|pos| pos.windows(2).all(|w| w[0] <= w[1]));
    check(
        "player-playback-ordered",
        after.contains(&"playback") && order_ok,
        format!("play+tick fires {}", after.join(",")),
    );

    // ---- headless edit round-trip with exact undo+redo ------------------
    let mut edit = open(&dir);
    let before = edit.timeline_rows_human();
    let applied = match edit.apply(EditOp::MoveElement {
        target: ClipRef::by_id("c"),
        to: 0,
    }) {
        Ok(a) => a,
        Err(e) => fail("edit-apply", format!("refused: {e}")),
    };
    let moved = edit.timeline_rows_human();
    check(
        "edit-apply-visible",
        moved != before,
        format!("move stages seq {}", applied.seq),
    );
    let undo = match edit.undo() {
        Ok(u) => u,
        Err(e) => fail("edit-undo", format!("refused: {e}")),
    };
    check(
        "edit-undo-exact",
        edit.timeline_rows_human() == before && undo.redo_available,
        format!("undo reverses seq {} ({})", undo.seq, undo.summary),
    );
    let redo = match edit.redo() {
        Ok(r) => r,
        Err(e) => fail("edit-redo", format!("refused: {e}")),
    };
    check(
        "edit-redo-exact",
        edit.timeline_rows_human() == moved && redo.seq == undo.seq,
        format!("redo re-applies seq {} ({})", redo.seq, redo.summary),
    );

    // ---- variables: declare, validate, scan -----------------------------
    let mut vars = open(&dir);
    let disk_offenders = vars.validate_variables();
    check(
        "var-validate-clean",
        disk_offenders.is_empty(),
        "on-disk declarations all bound".to_string(),
    );
    let declared = match vars.declare_variable(VariableInput {
        id: "tagline".to_string(),
        var_type: "string".to_string(),
        default: Some("Fresh daily".to_string()),
        label: None,
        options: None,
    }) {
        Ok(d) => d,
        Err(e) => fail("var-declare", format!("refused: {e}")),
    };
    check(
        "var-declare",
        declared.id == "tagline" && declared.var_type == "string",
        format!("staged --{} ({})", declared.id, declared.var_type),
    );
    let dup = vars.declare_variable(VariableInput {
        id: "title".to_string(),
        var_type: "string".to_string(),
        default: Some("Dup".to_string()),
        label: None,
        options: None,
    });
    check(
        "var-declare-refuses-duplicate",
        dup.is_err(),
        format!("duplicate refused: {}", dup.err().unwrap()),
    );
    let offenders = vars.validate_variables();
    let flags_unbound = offenders.len() == 1
        && offenders[0].rule == "E_BAD_VARIABLE"
        && offenders[0].message.contains("--tagline");
    for o in &offenders {
        println!("VAROFFENDER {} {} {}", o.rule, o.file, o.message);
    }
    check(
        "var-validate-flags-unbound",
        flags_unbound,
        format!("staged --tagline reports {}", offenders.len()),
    );
    let uses = vars.scan_variable_usage(None);
    let names_found: Vec<&str> = uses.iter().map(|u| u.variable.as_str()).collect();
    let hyphen = uses
        .iter()
        .any(|u| u.attribute.contains('-') && u.file == "composition.kdl");
    for u in &uses {
        println!(
            "VARUSE {} {}:{}:{} {}",
            u.variable, u.file, u.line, u.column, u.attribute
        );
    }
    check(
        "var-scan",
        ["title", "accent", "show", "step", "brand"]
            .iter()
            .all(|want| names_found.contains(want))
            && hyphen,
        format!("{} use(s), hyphenated attributes kept", uses.len()),
    );

    // ---- timeline: plan counts, apply receipts, verb undo ----------------
    let mut tl = open(&dir);
    let tl_before = tl.timeline_rows_human();
    let verb = TimelineVerb::Move {
        target: ClipRef::by_id("b"),
        to: TimeExpr::Offset(1.0),
    };
    let plan = match tl.timeline_plan(&verb) {
        Ok(p) => p,
        Err(e) => fail("timeline-plan", format!("refused: {e}")),
    };
    check(
        "timeline-plan-count",
        plan.affected == 1
            && plan.to_string() == "timeline plan: 1 clip(s) would change"
            && tl.timeline_rows_human() == tl_before,
        format!("{plan}"),
    );
    let receipt = match tl.timeline_apply(verb) {
        Ok(r) => r,
        Err(e) => fail("timeline-apply", format!("refused: {e}")),
    };
    check(
        "timeline-apply-receipt",
        receipt.applied == 1 && receipt.seqs.len() == 1 && receipt.undo_available,
        format!("{} (seqs {:?})", receipt.verb_summary, receipt.seqs),
    );
    let vundo = match tl.timeline_undo() {
        Ok(u) => u,
        Err(e) => fail("timeline-undo", format!("refused: {e}")),
    };
    check(
        "timeline-undo-exact",
        tl.timeline_rows_human() == tl_before,
        format!("verb undo reverses {} patch(es)", vundo.undone),
    );

    // ---- guarded interleaved history turns ------------------------------
    let mut maker = open(&dir);
    if maker
        .apply(EditOp::SetLabel {
            target: ClipRef::by_id("a"),
            label: Some("maker note".to_string()),
        })
        .is_err()
    {
        fail("history-maker-stage", "maker edit refused".to_string());
    }
    let maker_turn = match maker.persist_turn("maker") {
        Ok(t) => t,
        Err(e) => fail("history-maker-turn", format!("refused: {e}")),
    };
    let mut agent = open(&dir);
    if agent
        .apply(EditOp::SetLabel {
            target: ClipRef::by_id("b"),
            label: Some("agent note".to_string()),
        })
        .is_err()
    {
        fail("history-agent-stage", "agent edit refused".to_string());
    }
    let agent_turn = match agent.persist_turn("agent") {
        Ok(t) => t,
        Err(e) => fail("history-agent-turn", format!("refused: {e}")),
    };
    check(
        "history-turns-recorded",
        maker_turn.turn.is_some() && agent_turn.turn.is_some(),
        format!(
            "turns {:?}/{:?} by {:?}/{:?}",
            maker_turn.turn, agent_turn.turn, maker_turn.actor, agent_turn.actor
        ),
    );
    let holder = begin_turn(
        &dir,
        "holder",
        &BeginOptions {
            exclusive: true,
            if_open: false,
        },
    );
    check(
        "history-guard-begin",
        holder.is_ok(),
        "holder takes an exclusive turn".to_string(),
    );
    let mut writer = open(&dir);
    if writer
        .apply(EditOp::SetLabel {
            target: ClipRef::by_id("c"),
            label: Some("writer note".to_string()),
        })
        .is_err()
    {
        fail("history-writer-stage", "writer edit refused".to_string());
    }
    let refused = writer.persist_turn("writer");
    check(
        "history-guard-held",
        refused.is_err_and(|e| e.code == E_SDK_GUARD_HELD),
        "persist under a foreign exclusive turn refuses E_SDK_GUARD_HELD".to_string(),
    );
    if end_turn(&dir, "holder").is_err() {
        fail(
            "history-guard-end",
            "ending the holder turn refused".to_string(),
        );
    }
    let mut writer = open(&dir);
    if writer
        .apply(EditOp::SetLabel {
            target: ClipRef::by_id("c"),
            label: Some("writer note".to_string()),
        })
        .is_err()
    {
        fail(
            "history-writer-retry",
            "writer edit refused after release".to_string(),
        );
    }
    let writer_turn = match writer.persist_turn("writer") {
        Ok(t) => t,
        Err(e) => fail("history-writer-turn", format!("refused: {e}")),
    };
    check(
        "history-guard-released",
        writer_turn.turn.is_some(),
        "persist succeeds once the turn is ended".to_string(),
    );
    let listing = match list_turns(&dir) {
        Ok(l) => l,
        Err(e) => fail("history-list", format!("refused: {e}")),
    };
    let actors: Vec<&str> = listing.turns.iter().map(|t| t.actor.as_str()).collect();
    check(
        "history-interleaved-order",
        actors == ["maker", "agent", "writer"],
        format!("turns interleave {}", actors.join(",")),
    );

    println!("sdk {}", env!("CARGO_PKG_VERSION"));
    println!("PROOF: PASS");
}
