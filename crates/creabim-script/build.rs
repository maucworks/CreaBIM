//! Link the bundled libpython: PyO3's build script reads include
//! flags from `PYO3_PYTHON`, but its sysconfig libdir (`/install/lib`)
//! is a build-time path that does not exist here. This script adds
//! the real lib dir of the provisioned bundle to the linker search
//! path. Provisioning itself stays an explicit step
//! (`cargo run -p creabim-provision`, or just `make setup`):
//! no network I/O in the build.
//!
//! The `PYO3_PYTHON` check below is the guardrail: without it PyO3
//! silently falls back to whatever Python the build machine happens
//! to have, links it, and the binary dies at startup with a dyld
//! error far from the cause. Fail here instead, loudly.

fn main() {
    println!("cargo:rerun-if-env-changed=PYO3_PYTHON");
    println!("cargo:rerun-if-env-changed=CREABIM_PYTHON_HOME");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("PYO3_PYTHON").is_err() {
        panic!(
            "PYO3_PYTHON is not set, so PyO3 would silently link the system Python. Either `export PYO3_PYTHON=<home>/bin/python3.13` (run `cargo run -p creabim-provision` first to fetch the bundle), or just use the Makefile targets (e.g. `make build`), which set it up."
        );
    }
    let home = creabim_provision::home_under(&creabim_provision::cache_dir());
    let lib = python_lib_dir(&home);
    if !lib.is_dir() {
        panic!(
            "bundled libpython not found at {lib:?}: run `cargo run -p creabim-provision` first, then rebuild"
        );
    }
    println!("cargo:rustc-link-search=native={}", lib.display());
}

#[cfg(target_os = "windows")]
fn python_lib_dir(home: &std::path::Path) -> std::path::PathBuf {
    home.join("libs")
}

#[cfg(not(target_os = "windows"))]
fn python_lib_dir(home: &std::path::Path) -> std::path::PathBuf {
    home.join("lib")
}
