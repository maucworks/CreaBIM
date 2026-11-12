//! Interpreter lifecycle: boot the bundled CPython once per process.
//!
//! The interpreter is initialized once and never finalized (CPython
//! rule); `ensure_initialized` is idempotent for the same home and
//! an error for a different one. Full `PyConfig` isolated mode is a
//! follow-up; the env vars below already cut off user site packages.

use pyo3::prelude::*;
use pyo3::PyErr;
use std::path::Path;
use std::sync::OnceLock;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error(transparent)]
    Provision(#[from] creabim_provision::ProvisionError),
    #[error("interpreter was already initialized with a different home")]
    AlreadyInitialized,
    #[error("python error: {0}")]
    Python(String),
}

static HOME: OnceLock<String> = OnceLock::new();

/// Boot the bundled interpreter; returns `sys.version`.
pub fn ensure_initialized(home: &Path) -> Result<String, ScriptError> {
    let home_str = home.to_string_lossy().into_owned();
    if let Some(first) = HOME.get() {
        if *first != home_str {
            return Err(ScriptError::AlreadyInitialized);
        }
    } else {
        // Process-wide by design: the interpreter reads these once,
        // before first initialization (which happens on first attach
        // below, via the `auto-initialize` feature).
        std::env::set_var("PYTHONHOME", &home_str);
        std::env::set_var("PYTHONNOUSERSITE", "1");
        Python::initialize();
        let _ = HOME.set(home_str);
    }
    Python::attach(|py| {
        let version: String = py
            .import("sys")
            .and_then(|sys| sys.getattr("version"))
            .and_then(|v| v.extract())
            .map_err(|e: PyErr| ScriptError::Python(e.to_string()))?;
        Ok(version)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boots_bundled_interpreter_and_reports_pinned_version() {
        let home = crate::ensure_provisioned().expect("provision the bundle");
        let version = ensure_initialized(&home).expect("boot the interpreter");
        assert!(
            version.starts_with(crate::PINNED_PYTHON),
            "unexpected interpreter: {version}"
        );
    }
}
