//! The `creabim` Python builder package (M-S7b): typed constructors
//! for script op dicts, vendored as source and written into the
//! app-managed site-packages at run time.
//!
//! The package travels *with the binary*, not with pip: `include_str!`
//! bakes the sources in, and [`ensure_api`] writes them out when they
//! differ from what's installed. Version lock is therefore exact (the
//! builders always match the parser in the same binary), offline
//! first runs keep working, and no repository path is needed at run
//! time. The engine already puts the site dir on `sys.path` at boot,
//! so scripts `from creabim import door` with no further setup.

const PACKAGE: &[(&str, &str)] = &[(
    "creabim/__init__.py",
    include_str!("../python/creabim/__init__.py"),
)];

/// Write the builder package into the provisioned site dir when it is
/// missing or stale. Pure file compare — the package is small and the
/// write only happens when content actually changed.
pub fn ensure_api(home: &std::path::Path) -> Result<(), String> {
    let site = creabim_provision::site_dir_for(home);
    for (rel, source) in PACKAGE {
        let path = site.join(rel);
        let current = std::fs::read(&path).ok();
        if current.as_deref() == Some(source.as_bytes()) {
            continue;
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, source).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("creabim-api-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("python")).unwrap();
        dir.join("python")
    }

    #[test]
    fn writes_once_then_goes_quiet() {
        let home = tempdir("install");
        ensure_api(&home).unwrap();
        let init = creabim_provision::site_dir_for(&home).join("creabim/__init__.py");
        assert!(init.exists());
        let before = std::fs::read(&init).unwrap();
        ensure_api(&home).unwrap();
        assert_eq!(std::fs::read(&init).unwrap(), before);
    }
}
