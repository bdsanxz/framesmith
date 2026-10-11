//! Builds the in-page runtime to WebAssembly so it can be embedded.
//!
//! This is the one approved web build step: our own Rust, compiled for the
//! page. It runs a second cargo with its own target folder inside OUT_DIR.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by cargo"));
    let here = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo"));
    let workspace = here.join("../..");
    for input in ["runtime", "model", "motion", "timebase", "protocol"] {
        println!("cargo:rerun-if-changed={}", here.join("..").join(input).join("src").display());
        println!(
            "cargo:rerun-if-changed={}",
            here.join("..").join(input).join("Cargo.toml").display()
        );
    }
    println!("cargo:rerun-if-changed={}", workspace.join("Cargo.toml").display());
    println!("cargo:rerun-if-changed={}", workspace.join("Cargo.lock").display());

    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let target_dir = out.join("wasm-target");
    let mut command = Command::new(cargo);
    command
        .current_dir(&workspace)
        .args([
            "build",
            "--package",
            "runtime",
            "--lib",
            "--target",
            "wasm32-unknown-unknown",
            "--profile",
            "wasm",
        ])
        .arg("--target-dir")
        .arg(&target_dir);
    // Settings meant for the outer build must not leak into this one.
    for name in [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_TARGET_DIR",
        "CARGO_BUILD_TARGET",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
    ] {
        command.env_remove(name);
    }
    let status = command.status().expect("cargo could not be started to build the in-page runtime");
    assert!(
        status.success(),
        "the in-page runtime did not build for WebAssembly. \
         The wasm32-unknown-unknown target must be installed: `rustup target add wasm32-unknown-unknown`."
    );
    let built = target_dir.join("wasm32-unknown-unknown/wasm/runtime.wasm");
    std::fs::copy(&built, out.join("runtime.wasm")).expect("the built runtime could not be copied");
}
