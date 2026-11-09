//! `catalog search|show|report-miss|meaning-search|meaning-revoke`
//! command implementations (Sprint 19, T005 / CAP-005 and T006 /
//! CAP-006).
//!
//! Word search ranks the merged registry index offline: every
//! surface builds one [`catalog::SearchIndex`] from the same
//! listing and ranks through the same search, so the CLI, the
//! picker (the numbered human listing), and the Studio panel agree.
//! An empty index reports the empty registry instead of zero hits;
//! a search with words but no hits offers search-miss reporting.
//!
//! Nothing here touches the network or writes into the registry:
//! `report-miss` only appends to the miss-report log, and every
//! other verb is read-only.

use std::path::PathBuf;

/// Usage error for the `catalog` verbs: prints usage, exits 2 like
/// every other CLI usage error.
fn catalog_usage_error(msg: &str) -> ! {
    eprintln!("framesmith catalog: {msg}.");
    eprintln!(
        "usage: framesmith catalog search <words...> [--json] [--registry <dir>]\n\
         \x20       framesmith catalog show <name> [--json] [--registry <dir>]\n\
         \x20       framesmith catalog report-miss <words...> [--registry <dir>] [--out <file>]\n\
         \x20       framesmith catalog install <project> <name...> [--tag <tag>...] [--registry <dir>] [--vars <id=value>...] [--force]\n\
         \x20       framesmith catalog meaning-search <words...> [--json] [--registry <dir>] [--consent yes|no]\n\
         \x20       framesmith catalog meaning-revoke"
    );
    std::process::exit(2);
}

/// Entry point for `framesmith catalog ...`: parse the verb and run
/// it. Returns the process exit code.
pub fn cmd_catalog(args: &[String]) -> i32 {
    if args.is_empty() {
        catalog_usage_error(
            "expected a verb: search, show, report-miss, install, meaning-search, or meaning-revoke",
        );
    }
    match args[0].as_str() {
        "search" => {
            let (words, json, registry) = parse_search_args(&args[1..]);
            if words.is_empty() {
                catalog_usage_error("search needs at least one word");
            }
            cmd_catalog_search(&words, json, registry.as_deref())
        }
        "show" => {
            let (name, json, registry) = parse_show_args(&args[1..]);
            cmd_catalog_show(&name, json, registry.as_deref())
        }
        "report-miss" => {
            let (words, registry, out) = parse_report_args(&args[1..]);
            if words.is_empty() {
                catalog_usage_error("report-miss needs at least one word");
            }
            cmd_catalog_report_miss(&words, registry.as_deref(), out.as_deref())
        }
        "install" => {
            let (project, names, tags, registry, vars, force) = parse_install_args(&args[1..]);
            if names.is_empty() && tags.is_empty() {
                catalog_usage_error("install needs at least one <name> or --tag");
            }
            cmd_catalog_install(&project, &names, &tags, registry.as_deref(), &vars, force)
        }
        "meaning-search" => {
            let (words, json, registry, consent) = parse_meaning_args(&args[1..]);
            if words.is_empty() {
                catalog_usage_error("meaning-search needs at least one word");
            }
            cmd_catalog_meaning_search(&words, json, registry.as_deref(), consent)
        }
        "meaning-revoke" => {
            if !args[1..].is_empty() {
                catalog_usage_error("meaning-revoke takes no arguments");
            }
            cmd_catalog_meaning_revoke()
        }
        other => catalog_usage_error(&format!("unknown catalog verb {other:?}")),
    }
}

fn parse_search_args(args: &[String]) -> (Vec<String>, bool, Option<String>) {
    let mut words = Vec::new();
    let mut json = false;
    let mut registry: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                if json {
                    catalog_usage_error("duplicate --json flag");
                }
                json = true;
            }
            "--registry" => {
                i += 1;
                if i >= args.len() {
                    catalog_usage_error("--registry needs a value (a registry directory)");
                }
                if registry.is_some() {
                    catalog_usage_error("duplicate --registry flag");
                }
                registry = Some(args[i].clone());
            }
            other if other.starts_with('-') => {
                catalog_usage_error(&format!("unknown flag {other:?}"));
            }
            other => words.push(other.to_string()),
        }
        i += 1;
    }
    (words, json, registry)
}

fn parse_show_args(args: &[String]) -> (String, bool, Option<String>) {
    let mut name: Option<String> = None;
    let mut json = false;
    let mut registry: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                if json {
                    catalog_usage_error("duplicate --json flag");
                }
                json = true;
            }
            "--registry" => {
                i += 1;
                if i >= args.len() {
                    catalog_usage_error("--registry needs a value (a registry directory)");
                }
                if registry.is_some() {
                    catalog_usage_error("duplicate --registry flag");
                }
                registry = Some(args[i].clone());
            }
            other if other.starts_with('-') => {
                catalog_usage_error(&format!("unknown flag {other:?}"));
            }
            other => {
                if name.is_some() {
                    catalog_usage_error(&format!(
                        "expected exactly one <name>, found extra {other:?}"
                    ));
                }
                name = Some(other.to_string());
            }
        }
        i += 1;
    }
    match name {
        Some(n) => (n, json, registry),
        None => catalog_usage_error("show needs exactly one <name>"),
    }
}

fn parse_report_args(args: &[String]) -> (Vec<String>, Option<String>, Option<String>) {
    let mut words = Vec::new();
    let mut registry: Option<String> = None;
    let mut out: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--registry" => {
                i += 1;
                if i >= args.len() {
                    catalog_usage_error("--registry needs a value (a registry directory)");
                }
                if registry.is_some() {
                    catalog_usage_error("duplicate --registry flag");
                }
                registry = Some(args[i].clone());
            }
            "--out" => {
                i += 1;
                if i >= args.len() {
                    catalog_usage_error("--out needs a value (a report file)");
                }
                if out.is_some() {
                    catalog_usage_error("duplicate --out flag");
                }
                out = Some(args[i].clone());
            }
            other if other.starts_with('-') => {
                catalog_usage_error(&format!("unknown flag {other:?}"));
            }
            other => words.push(other.to_string()),
        }
        i += 1;
    }
    (words, registry, out)
}

/// Custom registry sources for read-only verbs (`search`, `show`):
/// the project configuration in the current directory names extra
/// registries merged with the bundled base. A missing configuration
/// means bundled only; a misshapen one warns (naming the file) and
/// keeps the search working on the bundled base — installs still
/// refuse on bad configuration (see [`cmd_catalog_install`]).
fn custom_sources_for_cwd() -> (Vec<catalog::RegistrySource>, Vec<String>) {
    let cwd = std::path::PathBuf::from(".");
    match catalog::project_registries(&cwd) {
        Ok(customs) => (catalog::registry_sources(&cwd, &customs), Vec::new()),
        Err(error) => (
            Vec::new(),
            vec![format!(
                "project configuration names no usable custom registries: {error}"
            )],
        ),
    }
}

/// Merge warnings plus collision notes as human `warning:` lines on
/// stderr. Never used for `--json`: machine output stays parseable
/// and carries the same warnings inside its envelope instead.
fn emit_merge_notes(warnings: &[String], collisions: &[catalog::CollisionNote]) {
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
    for collision in collisions {
        eprintln!(
            "warning: collision on {:?}: {:?} wins over {} (configured order)",
            collision.name,
            collision.winner,
            collision
                .losers
                .iter()
                .map(|l| format!("{l:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

/// `catalog search`: rank the merged index offline and print the
/// human-friendly picker (numbered picks with each hit's registry of
/// origin) or the machine envelope with `--json`. Custom registries
/// from the current directory's project configuration merge into the
/// index; unreachable ones warn by name and skipped invalid manifests
/// name the file while the bundled base keeps working. Exit 0 on
/// hits and on no-hit offers; exit 1 when the index is empty,
/// reporting the empty registry.
pub fn cmd_catalog_search(words: &[String], json: bool, registry: Option<&str>) -> i32 {
    let dir = catalog::resolve_registry_dir(registry);
    let base = catalog::RegistrySource {
        name: catalog::BUNDLED_ORIGIN.to_string(),
        dir: dir.clone(),
        before_bundled: false,
    };
    let (extras, mut warnings) = custom_sources_for_cwd();
    let (listing, index) = catalog::load_merged_search_index(&base, &extras);
    warnings.extend(listing.warnings.iter().cloned());
    let query = words.join(" ");
    if index.is_empty() {
        if json {
            println!("{}", catalog::empty_json_with_warnings(&dir, &warnings));
        } else {
            emit_merge_notes(&warnings, &listing.collisions);
            eprintln!("{}", catalog::empty_registry_report(&dir));
        }
        return 1;
    }
    let hits = index.search(&query);
    if hits.is_empty() {
        if json {
            println!(
                "{}",
                catalog::no_hits_json_with_warnings(&query, &dir, &warnings)
            );
        } else {
            emit_merge_notes(&warnings, &listing.collisions);
            println!("{}", catalog::no_hits_report(&query, &dir));
        }
        return 0;
    }
    if json {
        println!(
            "{}",
            catalog::hits_json_with_warnings(&query, &dir, &hits, &warnings)
        );
    } else {
        emit_merge_notes(&warnings, &listing.collisions);
        print!("{}", catalog::render_picker(&query, &dir, &hits));
    }
    0
}

/// `catalog show`: one item page with its manifest fields, its
/// registry of origin, variables, and the copyable install handoff
/// (INT-001). Custom registries merge in like search; unknown names
/// refuse naming the item and the merged registry holdings.
pub fn cmd_catalog_show(name: &str, json: bool, registry: Option<&str>) -> i32 {
    let dir = catalog::resolve_registry_dir(registry);
    let base = catalog::RegistrySource {
        name: catalog::BUNDLED_ORIGIN.to_string(),
        dir: dir.clone(),
        before_bundled: false,
    };
    let (extras, mut warnings) = custom_sources_for_cwd();
    let (listing, _index) = catalog::load_merged_search_index(&base, &extras);
    warnings.extend(listing.warnings.iter().cloned());
    if listing.items.is_empty() {
        if json {
            println!("{}", catalog::empty_json_with_warnings(&dir, &warnings));
        } else {
            emit_merge_notes(&warnings, &listing.collisions);
            eprintln!("{}", catalog::empty_registry_report(&dir));
        }
        return 1;
    }
    let Some(item) = listing.find(name) else {
        let available: Vec<String> = listing.names().iter().map(|s| s.to_string()).collect();
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "name": name,
                    "found": false,
                    "registry": dir.display().to_string(),
                    "available": available,
                    "warnings": warnings,
                })
            );
        } else {
            emit_merge_notes(&warnings, &listing.collisions);
            eprintln!(
                "framesmith catalog: unknown item {name:?} in the registry at {}.\n\
                 available: {}.\n\
                 fix: pass one of those names; nothing was installed.",
                dir.display(),
                available
                    .iter()
                    .map(|n| format!("{n:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        return 1;
    };
    let manifest = &item.manifest;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "name": manifest.name,
                "type": manifest.item_type.as_str(),
                "title": manifest.title,
                "description": manifest.description,
                "tags": manifest.tags,
                "files": manifest.files.iter().map(|f| serde_json::json!({
                    "src": f.src,
                    "target": f.target,
                })).collect::<Vec<_>>(),
                "width": manifest.width,
                "height": manifest.height,
                "duration": manifest.duration,
                "found": true,
                "registry": dir.display().to_string(),
                "origin": item.origin,
                "warnings": warnings,
                "install": format!("framesmith catalog install <project> {:?}", manifest.name),
            })
        );
        return 0;
    }
    emit_merge_notes(&warnings, &listing.collisions);
    let mut out = String::new();
    out.push_str(&format!(
        "item {} ({}) from {:?} in the registry at {}:\n",
        manifest.name,
        manifest.item_type,
        item.origin,
        dir.display()
    ));
    out.push_str(&format!("  title: {}\n", manifest.title));
    out.push_str(&format!("  description: {}\n", manifest.description));
    if manifest.tags.is_empty() {
        out.push_str("  tags: (none)\n");
    } else {
        out.push_str(&format!("  tags: {}\n", manifest.tags.join(", ")));
    }
    out.push_str("  files:\n");
    for f in &manifest.files {
        out.push_str(&format!("    {} -> {}\n", f.src, f.target));
    }
    match (manifest.width, manifest.height, manifest.duration) {
        (Some(w), Some(h), Some(d)) => {
            out.push_str(&format!("  canvas: {w}x{h}, duration {d}s\n"));
        }
        _ => {}
    }
    out.push_str(&format!(
        "  install: framesmith catalog install <project> {:?}\n",
        manifest.name
    ));
    print!("{out}");
    0
}

/// Parse `catalog install <project> <name...> [--tag <tag>...]
/// [--registry <dir>] [--vars <id=value>...] [--force]`: the first
/// positional is the project directory, the rest are exact item
/// names; `--tag` and `--vars` repeat, `--force` restores registry
/// bytes over local edits on reinstall. At least one name or tag is
/// required (checked by the caller so usage stays exit 2).
fn parse_install_args(
    args: &[String],
) -> (
    String,
    Vec<String>,
    Vec<String>,
    Option<String>,
    Vec<String>,
    bool,
) {
    let mut project: Option<String> = None;
    let mut names: Vec<String> = Vec::new();
    let mut tags: Vec<String> = Vec::new();
    let mut registry: Option<String> = None;
    let mut vars: Vec<String> = Vec::new();
    let mut force = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--tag" => {
                i += 1;
                if i >= args.len() {
                    catalog_usage_error("--tag needs a value (a registry tag)");
                }
                tags.push(args[i].clone());
            }
            "--registry" => {
                i += 1;
                if i >= args.len() {
                    catalog_usage_error("--registry needs a value (a registry directory)");
                }
                if registry.is_some() {
                    catalog_usage_error("duplicate --registry flag");
                }
                registry = Some(args[i].clone());
            }
            "--vars" => {
                i += 1;
                if i >= args.len() {
                    catalog_usage_error("--vars needs a value (an id=value assignment)");
                }
                vars.push(args[i].clone());
            }
            "--force" => {
                if force {
                    catalog_usage_error("duplicate --force flag");
                }
                force = true;
            }
            other if other.starts_with('-') => {
                catalog_usage_error(&format!("unknown flag {other:?}"));
            }
            other => {
                if project.is_none() {
                    project = Some(other.to_string());
                } else {
                    names.push(other.to_string());
                }
            }
        }
        i += 1;
    }
    match project {
        Some(p) => (p, names, tags, registry, vars, force),
        None => catalog_usage_error("install needs a <project> plus at least one <name> or --tag"),
    }
}

/// `catalog install`: resolve names and tags as one set over the
/// merged registries, gate CLI versions, echo the resolved order with
/// adopted-audio labels and each item's registry of origin (INT-001),
/// then write with `--vars` baking and the SHA256 lock. The project
/// configuration names custom registries merged with the bundled
/// base; unreachable ones warn by name and skipped invalid manifests
/// name the file while the bundled base keeps working. Resolution,
/// gates, and validation run fully before the first write; any
/// refusal names the item, dependency, tag, target, version, or
/// variable and writes nothing (exit 1, REC-001/REC-002/REC-004).
pub fn cmd_catalog_install(
    project: &str,
    names: &[String],
    tags: &[String],
    registry: Option<&str>,
    vars: &[String],
    force: bool,
) -> i32 {
    let dir = catalog::resolve_registry_dir(registry);
    let project_dir = std::path::PathBuf::from(project);
    let selection = catalog::Selection {
        names: names.to_vec(),
        tags: tags.to_vec(),
    };
    let fail = |error: &catalog::InstallError| -> i32 {
        eprintln!("framesmith catalog install refused: {error}.");
        if error.wrote_nothing() {
            eprintln!("fix: check names, tags, and targets above; nothing was written.");
        } else {
            eprintln!(
                "fix: repair the location above and rerun; the install stopped at that file."
            );
        }
        1
    };
    // Options first: malformed or duplicate `--vars` assignments
    // refuse before anything resolves.
    let options = match catalog::InstallOptions::with_vars(env!("CARGO_PKG_VERSION"), vars, force) {
        Ok(options) => options,
        Err(error) => return fail(&error),
    };
    // Remaps first so the echoed order shows the final targets; a bad
    // project configuration refuses before anything resolves.
    let remaps = match catalog::project_remaps(&project_dir) {
        Ok(remaps) => remaps,
        Err(error) => return fail(&error),
    };
    // Custom registries second: a misshapen `registries` value refuses
    // naming the configuration file, like remaps do.
    let extras = match catalog::project_registries(&project_dir) {
        Ok(customs) => catalog::registry_sources(&project_dir, &customs),
        Err(error) => return fail(&error),
    };
    let merged = match catalog::resolve_merged(&dir, &extras, &selection) {
        Ok(merged) => merged,
        Err(error) => return fail(&error),
    };
    let plan = &merged.plan;
    // Gates before the echo: a gated set refuses naming item,
    // required, and current versions with zero writes.
    if let Err(error) = catalog::check_version_gates(plan, &options.current_version) {
        return fail(&error);
    }
    emit_merge_notes(&merged.warnings, &merged.collisions);
    println!("install order: {}", plan.order().join(" -> "));
    for (entry, origin) in plan.items.iter().zip(merged.origins.iter()) {
        let mut files: Vec<String> = Vec::new();
        for file in &entry.manifest.files {
            let target = catalog::remap_target(&file.target, &remaps);
            let label = match file.audio {
                Some(role) if catalog::is_audio_target(&file.target) => {
                    format!("{target} as {}", role.as_str())
                }
                _ => target,
            };
            files.push(label);
        }
        // Bundled items keep the long-standing `name: files` echo;
        // custom origins print beside the name so merged installs stay
        // origin-truthful without changing the default surface.
        if origin == catalog::BUNDLED_ORIGIN {
            println!("  {}: {}", entry.manifest.name, files.join(", "));
        } else {
            println!(
                "  {} [{}]: {}",
                entry.manifest.name,
                origin,
                files.join(", ")
            );
        }
    }
    for adopted in catalog::adopted_audio(&plan, &remaps) {
        println!(
            "  adopted audio: {} ({} as {})",
            adopted.target,
            adopted.item,
            adopted.role.as_str()
        );
    }
    match catalog::write_plan_with_options(&plan, &project_dir, &options) {
        Ok(report) => {
            println!(
                "installed {} item(s), {} file(s) into {}.",
                report.items.len(),
                report.files.len(),
                project_dir.display()
            );
            if !report.vars_baked.is_empty() {
                println!("baked vars: {}.", report.vars_baked.join(", "));
            }
            if !report.kept.is_empty() {
                println!(
                    "kept {} edited file(s): {}.",
                    report.kept.len(),
                    report.kept.join(", ")
                );
            }
            if !report.forced.is_empty() {
                println!(
                    "force-restored {} file(s): {}.",
                    report.forced.len(),
                    report.forced.join(", ")
                );
            }
            if let Some(snippet) = report.mount_snippet {
                println!("{snippet}");
            }
            0
        }
        Err(error) => fail(&error),
    }
}

/// `catalog report-miss`: store one search-miss report. A report
/// that cannot be stored says so and keeps the search result (exit
/// 0 either way): the miss log must never block searching.
pub fn cmd_catalog_report_miss(words: &[String], registry: Option<&str>, out: Option<&str>) -> i32 {
    let query = words.join(" ");
    let stored = match out {
        Some(path) => {
            let path = PathBuf::from(path);
            let line = serde_json::json!({
                "query": query,
                "at": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            })
            .to_string()
                + "\n";
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .and_then(|mut f| {
                    use std::io::Write;
                    f.write_all(line.as_bytes())
                })
                .map(|()| path.clone())
                .map_err(|e| format!("cannot store the miss report at {}: {e}", path.display()))
        }
        None => {
            let dir = catalog::resolve_registry_dir(registry);
            catalog::append_miss_report(&dir, &query)
        }
    };
    match stored {
        Ok(path) => {
            println!(
                "catalog: recorded the miss {query:?} at {}.",
                path.display()
            );
            0
        }
        Err(message) => {
            println!("catalog: {message}; the search result stands.");
            0
        }
    }
}

/// Parse `catalog meaning-search <words...> [--json]
/// [--registry <dir>] [--consent yes|no]`: words plus the explicit
/// consent choice. No `--consent` flag is the first ask: explain the
/// download, request consent, answer with word results.
fn parse_meaning_args(
    args: &[String],
) -> (Vec<String>, bool, Option<String>, catalog::ConsentAnswer) {
    let mut words = Vec::new();
    let mut json = false;
    let mut registry: Option<String> = None;
    let mut consent = catalog::ConsentAnswer::Ask;
    let mut consent_seen = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => {
                if json {
                    catalog_usage_error("duplicate --json flag");
                }
                json = true;
            }
            "--registry" => {
                i += 1;
                if i >= args.len() {
                    catalog_usage_error("--registry needs a value (a registry directory)");
                }
                if registry.is_some() {
                    catalog_usage_error("duplicate --registry flag");
                }
                registry = Some(args[i].clone());
            }
            "--consent" => {
                i += 1;
                if i >= args.len() {
                    catalog_usage_error("--consent needs a value (yes or no)");
                }
                if consent_seen {
                    catalog_usage_error("duplicate --consent flag");
                }
                consent_seen = true;
                consent = match args[i].as_str() {
                    "yes" => catalog::ConsentAnswer::Yes,
                    "no" => catalog::ConsentAnswer::No,
                    other => catalog_usage_error(&format!(
                        "--consent expects yes or no, found {other:?}"
                    )),
                };
            }
            other if other.starts_with('-') => {
                catalog_usage_error(&format!("unknown flag {other:?}"));
            }
            other => words.push(other.to_string()),
        }
        i += 1;
    }
    (words, json, registry, consent)
}

/// `catalog meaning-search`: the consent-gated meaning surface
/// (T006 / CAP-006). Word hits print in every outcome; meaning hits
/// print only after consent plus a successful one-time download. No
/// model bytes move without `--consent yes`; a failed download
/// reports its cause on word results; exit 0 whenever a result
/// stands, exit 1 only for an empty registry.
pub fn cmd_catalog_meaning_search(
    words: &[String],
    json: bool,
    registry: Option<&str>,
    consent: catalog::ConsentAnswer,
) -> i32 {
    let dir = catalog::resolve_registry_dir(registry);
    let query = words.join(" ");
    let (listing, _index) = catalog::load_search_index(&dir);
    if listing.items.is_empty() {
        if json {
            println!("{}", catalog::empty_json(&dir));
        } else {
            eprintln!("{}", catalog::empty_registry_report(&dir));
        }
        return 1;
    }
    let state_dir = catalog::meaning_dir();
    let mut fetch = catalog::default_fetch;
    match catalog::meaning_search(&listing.items, &query, &state_dir, consent, &mut fetch) {
        catalog::MeaningOutcome::NeedsConsent { explanation, word } => {
            if json {
                println!("{}", catalog::needs_consent_json(&query, &dir, &word));
            } else {
                println!("{explanation}");
                println!("{}", catalog::render_word_fallback(&query, &dir, &word));
            }
            0
        }
        catalog::MeaningOutcome::Ranked { word, meaning } => {
            if json {
                println!(
                    "{}",
                    catalog::meaning_ranked_json(&query, &dir, &word, &meaning)
                );
            } else {
                print!(
                    "{}",
                    catalog::render_meaning_picker(&query, &dir, &meaning, &word)
                );
            }
            0
        }
        catalog::MeaningOutcome::Declined { notice, word } => {
            if json {
                println!("{}", catalog::declined_json(&query, &dir, &word));
            } else {
                println!("{notice}");
                println!("{}", catalog::render_word_fallback(&query, &dir, &word));
            }
            0
        }
        catalog::MeaningOutcome::DownloadFailed { cause, word } => {
            if json {
                println!(
                    "{}",
                    catalog::download_failed_json(&query, &dir, &cause, &word)
                );
            } else {
                println!("{}", catalog::download_failed_report(&cause, &query));
                println!("{}", catalog::render_word_fallback(&query, &dir, &word));
            }
            0
        }
    }
}

/// `catalog meaning-revoke`: delete the downloaded model and the
/// consent record, returning to word search. Idempotent.
pub fn cmd_catalog_meaning_revoke() -> i32 {
    let state_dir = catalog::meaning_dir();
    let removed = catalog::revoke_consent(&state_dir);
    println!("{}", catalog::revoked_report(removed));
    0
}
