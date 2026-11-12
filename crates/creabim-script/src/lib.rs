//! Scripting engine host: embedded CPython for CreaBIM script blocks.
//!
//! M-S0 spike: provision a pinned python-build-standalone distribution
//! and boot the interpreter in-process. The host API (model reads,
//! geometry builders, ops vocabulary) and the `ScriptEngine` trait
//! come next; this crate only proves interpreter provisioning and
//! linkage.
//!
//! Build-time setup is two steps (PyO3 links the interpreter it is
//! told about, so the bundle must exist first):
//!
//! ```sh
//! cargo run -p creabim-provision
//! PYO3_PYTHON=<home>/bin/python3.13 cargo test -p creabim-script
//! ```
//!
//! with `DYLD_LIBRARY_PATH=<home>/lib` (macOS) on the test command
//! so the loader finds the bundled libpython.

pub mod engine;
mod host;

pub use creabim_provision::{
    ensure as ensure_provisioned, ensure_packages, ProvisionError, PINNED_PYTHON, PINNED_TAG,
};
pub use engine::{ensure_initialized, ScriptError};
