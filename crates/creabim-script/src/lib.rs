//! Scripting engine host: embedded CPython for CreaBIM script blocks.
//!
//! M-S1: [`ScriptEngine`] runs a script object against a model
//! snapshot; [`run_to_transaction`] maps keyed outputs onto one undoable
//! transaction. Native op vocabulary only (walls, meshes).
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
pub mod run;

pub use creabim_provision::{
    ensure as ensure_provisioned, ensure_packages, home_under, site_dir_for, ProvisionError,
    PINNED_PYTHON, PINNED_TAG,
};
pub use engine::{async_interrupt, ensure_initialized, ScriptError};
pub use run::{
    resolve, run_to_transaction, CPythonEngine, ResolvedInputs, ResolvedValue, ScriptEngine,
    TracedRun,
};
