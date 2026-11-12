//! Link the bundled libpython: PyO3's build script reads include
//! flags from `PYO3_PYTHON`, but its sysconfig libdir (`/install/lib`)
//! is a build-time path that does not exist here. This script adds
//! the real lib dir of the provisioned bundle to the linker search
//! path. Provisioning itself stays an explicit step
//! (`cargo run -p creabim-provision`): no network I/O in the build.

fn main() {
    println!("cargo:rerun-if-env-changed=CREABIM_PYTHON_HOME");
    println!("cargo:rerun-if-changed=build.rs");
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
