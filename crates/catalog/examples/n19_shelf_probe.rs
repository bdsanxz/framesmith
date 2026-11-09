//! n19 proof probe (Sprint 19, T009): the two catalog surfaces the
//! shell proof script cannot reach through the CLI.
//!
//! Usage: `n19_shelf_probe <outdir>`
//!
//! - Prints the per-shelf counts (`format_shelf_counts`) to stdout,
//!   one line per shelf plus a total line.
//! - Writes the whole bundled registry to `<outdir>` (via
//!   `materialize_bundled`) so the shell gate can drive the real
//!   `framesmith catalog` verbs against real bundled content.
//! - Prints `coverage: clean` when `check_bundled_coverage` reports
//!   no faults; otherwise prints each fault and exits 1.

use std::path::PathBuf;

fn main() {
    let outdir = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("probe FAIL: usage: n19_shelf_probe <outdir>");
        std::process::exit(2);
    });
    let dir = PathBuf::from(&outdir);
    match catalog::materialize_bundled(&dir) {
        Ok(count) => println!("materialized {count} bundled items into {}", dir.display()),
        Err(error) => {
            eprintln!("probe FAIL: materialize_bundled: {error}");
            std::process::exit(1);
        }
    }
    print!("{}", catalog::format_shelf_counts(catalog::BUNDLED));
    let faults = catalog::check_bundled_coverage();
    if faults.is_empty() {
        println!("coverage: clean");
    } else {
        for fault in &faults {
            println!("coverage fault: {fault}");
        }
        eprintln!("probe FAIL: {} coverage fault(s)", faults.len());
        std::process::exit(1);
    }
}
