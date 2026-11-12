//! Interpreter lifecycle: boot the bundled CPython once per process.
//!
//! The interpreter is initialized once and never finalized (CPython
//! rule); `ensure_initialized` is idempotent for the same home and
//! an error for a different one. Full `PyConfig` isolated mode is a
//! follow-up; the env vars below already cut off user site packages.

use pyo3::prelude::*;
use pyo3::PyErr;
use std::path::Path;
use std::sync::{Once, OnceLock};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error(transparent)]
    Provision(#[from] creabim_provision::ProvisionError),
    #[error("interpreter was already initialized with a different home")]
    AlreadyInitialized,
    #[error("script op failed: {0}")]
    Op(String),
    #[error("unsupported engine: {0}")]
    UnsupportedEngine(String),
    #[error("python error: {0}")]
    Python(String),
}

static HOME: OnceLock<String> = OnceLock::new();
static INIT: Once = Once::new();

/// Boot the bundled interpreter; returns `sys.version`.
///
/// Order per call: provision the bundle, install packages, then
/// initialize. Registration must precede initialization (it panics
/// once running), and the site dir must exist before boot so the
/// `sys.path` entry below can resolve imports.
pub fn ensure_initialized(home: &Path) -> Result<String, ScriptError> {
    let home_str = home.to_string_lossy().into_owned();
    // Thread-safe: exactly one thread initializes, however many race it.
    INIT.call_once(|| {
        // Process-wide by design: the interpreter reads these once,
        // before first initialization.
        std::env::set_var("PYTHONHOME", &home_str);
        std::env::set_var("PYTHONNOUSERSITE", "1");
        crate::host::register();
        Python::initialize();
        let _ = HOME.set(home_str);
    });
    if HOME
        .get()
        .is_some_and(|first| *first != home.to_string_lossy())
    {
        return Err(ScriptError::AlreadyInitialized);
    }
    let site = creabim_provision::site_dir_for(home);
    Python::attach(|py| {
        // Idempotent: `s not in sys.path` guards repeat calls.
        let code = std::ffi::CString::new(format!(
            "import sys; s = r'{}'; s not in sys.path and sys.path.append(s)",
            site.display()
        ))
        .map_err(|e| ScriptError::Python(e.to_string()))?;
        py.run(&code, None, None)
            .map_err(|e| ScriptError::Python(e.to_string()))?;
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
