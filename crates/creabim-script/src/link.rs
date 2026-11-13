//! Disc-based script sources (M-S6): the linked file is the source of
//! truth when present; the embedded copy is the offline fallback.
//!
//! Convention over configuration, as Django's `main.app`: the model
//! holds the registration (the `ScriptObject`, with its name), and the
//! module is addressed by it — [`entrance_path`] derives
//! `scripts/<slug>.py` under the project directory. Scripts may import
//! helper modules from that directory (`import helpers`), so staleness
//! must cover the whole directory, not just the entry file (trap 15).

use creabim_core::{hash::StableHasher, ScriptObject};
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The project's dedicated script directory, next to the model file.
pub const SCRIPTS_DIR: &str = "scripts";

/// `<project>/scripts`.
pub fn scripts_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(SCRIPTS_DIR)
}

/// The project directory of a project *file* path (`house.creabim` ->
/// its parent), empty for a bare name (an unsaved project). A bare
/// name in the current directory resolves against the CWD, so the
/// CLI's plain `house.creabim` still finds its `scripts/` next door.
pub fn project_dir_of(project_file: &Path) -> Option<PathBuf> {
    let abs = std::path::absolute(project_file).unwrap_or_else(|_| project_file.to_path_buf());
    abs.parent()
        .map(|p| p.to_path_buf())
        .filter(|p| !p.as_os_str().is_empty())
}

/// Bring `file` under `scripts/` as `name`'s entrance point and return
/// the relative path to record on the element. A file already inside
/// `scripts/` is linked in place; one outside is copied in (the
/// original is left alone). Refuses to overwrite a different file
/// already at the entrance point.
pub fn adopt(project_dir: &Path, name: &str, file: &Path) -> Result<String, String> {
    let dir = scripts_dir(project_dir);
    let entrance = dir.join(format!("{}.py", creabim_core::script::slug(name)));
    let rel = format!(
        "{SCRIPTS_DIR}/{}",
        entrance.file_name().unwrap().to_string_lossy()
    );
    // Resolve the given path as written, cwd-relative or project-relative.
    let src = if file.is_absolute() || file.exists() {
        file.to_path_buf()
    } else {
        project_dir.join(file)
    };
    let same_file = match (src.canonicalize(), entrance.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    if !same_file {
        let bytes = std::fs::read(&src).map_err(|e| format!("{}: {e}", src.display()))?;
        if entrance.exists() && std::fs::read(&entrance).ok() != Some(bytes.clone()) {
            return Err(format!(
                "{} already holds a different script for {name:?}; move it away or rename the block",
                entrance.display()
            ));
        }
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(&entrance, bytes).map_err(|e| format!("{}: {e}", entrance.display()))?;
    }
    Ok(rel)
}

/// The linked file's content when it differs from the embedded source.
/// The block names a registry entry; the registry maps it onto a path.
/// No link or no entry: `Ok(None)` — the embedded copy is the
/// fallback, a shared model without its files still runs. Missing
/// file: `Ok(None)` too. An unreadable file is an error: loud, never
/// a silent fallback.
pub fn refresh(
    script: &ScriptObject,
    registry: Option<&creabim_core::ScriptRegistry>,
    project_dir: &Path,
) -> Result<Option<String>, String> {
    let rel = script
        .link
        .as_deref()
        .and_then(|name| registry?.path_of(name));
    let Some(rel) = rel else {
        return Ok(None);
    };
    let path = project_dir.join(rel);
    let text = match std::fs::read(&path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    if text == script.source {
        return Ok(None);
    }
    Ok(Some(text))
}

/// The registry path a block is linked against, if any.
pub fn linked_path<'a>(
    script: &'a ScriptObject,
    registry: Option<&'a creabim_core::ScriptRegistry>,
) -> Option<&'a str> {
    script
        .link
        .as_deref()
        .and_then(|name| registry?.path_of(name))
}

/// Write the embedded source out to the linked path (creating the
/// directory), so disc edits in an IDE see the app's edit too.
pub fn write_through(
    script: &ScriptObject,
    registry: Option<&creabim_core::ScriptRegistry>,
    project_dir: &Path,
) -> Result<(), String> {
    let Some(rel) = linked_path(script, registry) else {
        return Ok(());
    };
    let path = project_dir.join(rel);
    if std::fs::read(&path)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        == Some(script.source.clone())
    {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, &script.source).map_err(|e| format!("{}: {e}", path.display()))
}

/// Link an embedded block: move its current source out to the entrance
/// point (`scripts/<slug>.py`, never clobbering a different file) and
/// return the registry entry name plus the relative path to record.
/// The model edit (registry entry + block link) is the caller's, via
/// [`creabim_core::set_link_tx`].
pub fn link_from_embedded(
    project_dir: &Path,
    name: &str,
    script: &ScriptObject,
) -> Result<(String, String), String> {
    let entrance =
        scripts_dir(project_dir).join(format!("{}.py", creabim_core::script::slug(name)));
    if entrance.exists()
        && std::fs::read(&entrance)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .ok()
            != Some(script.source.clone())
    {
        return Err(format!(
            "{} already holds a different script; move it away or rename the block",
            entrance.display()
        ));
    }
    let rel = format!(
        "{SCRIPTS_DIR}/{}",
        entrance.file_name().unwrap().to_string_lossy()
    );
    let mut linked = script.clone();
    linked.link = Some(name.to_string());
    let mut registry = creabim_core::ScriptRegistry::default();
    registry.upsert(name, &rel);
    write_through(&linked, Some(&registry), project_dir)?;
    Ok((name.to_string(), rel))
}

/// Digest of every `*.py` under `scripts/`, sorted by path (FNV-1a over
/// path + content, per trap 14). A change in *any* helper module must
/// mark every script dirty — the entry source alone misses a changed
/// library. `None` when there is no scripts directory.
///
/// The mtime fingerprint (count + newest mtime) guards the re-read: the
/// app polls every frame, and stat is cheap where reads are not.
pub fn digest(project_dir: &Path) -> Option<String> {
    static CACHE: Mutex<Option<(Fingerprint, String)>> = Mutex::new(None);
    let dir = scripts_dir(project_dir);
    let mut files: Vec<(PathBuf, std::time::SystemTime, u64)> = Vec::new();
    for entry in std::fs::read_dir(&dir).ok()? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "py") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        files.push((
            path,
            meta.modified().unwrap_or(std::time::UNIX_EPOCH),
            meta.len(),
        ));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let fingerprint = Fingerprint {
        count: files.len() as u64,
        newest: files
            .iter()
            .map(|f| f.1)
            .max()
            .unwrap_or(std::time::UNIX_EPOCH),
        bytes: files.iter().map(|f| f.2).sum(),
    };
    if let Ok(cache) = CACHE.lock() {
        if let Some((seen, digest)) = cache.as_ref() {
            if *seen == fingerprint {
                return Some(digest.clone());
            }
        }
    }
    let mut h = StableHasher::new();
    for (path, _, _) in &files {
        let Ok(text) = std::fs::read(path) else {
            continue;
        };
        h.write(path.to_string_lossy().as_bytes());
        h.write(&text);
    }
    let digest = format!("{:016x}", h.finish());
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some((fingerprint, digest.clone()));
    }
    Some(digest)
}

#[derive(Clone, Copy, PartialEq)]
struct Fingerprint {
    count: u64,
    newest: std::time::SystemTime,
    bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::{ScriptObject, ScriptRegistry};

    fn tempdir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("creabim-link-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn script_at(name: &str, rel: &str, source: &str) -> (ScriptObject, ScriptRegistry) {
        let mut s = ScriptObject::new(source, "cpython");
        s.link = Some(name.into());
        let mut r = ScriptRegistry::default();
        r.upsert(name, rel);
        (s, r)
    }

    #[test]
    fn adopt_copies_in_and_links_in_place() {
        let dir = tempdir("adopt");
        let outside = dir.join("walls.py");
        std::fs::write(&outside, "# in: n: int = 1\nx = 1\n").unwrap();
        let rel = adopt(&dir, "Partitions", &outside).unwrap();
        assert_eq!(rel, "scripts/partitions.py");
        assert!(dir.join("scripts/partitions.py").exists());

        let inside = dir.join("scripts/edge.py");
        std::fs::write(&inside, "x = 2\n").unwrap();
        let rel = adopt(&dir, "Edge", &inside).unwrap();
        assert_eq!(rel, "scripts/edge.py");
        assert!(inside.exists());
    }

    #[test]
    fn adopt_refuses_to_clobber_a_different_script() {
        let dir = tempdir("clobber");
        let a = dir.join("a.py");
        std::fs::write(&a, "x = 1\n").unwrap();
        adopt(&dir, "Walls", &a).unwrap();
        let b = dir.join("b.py");
        std::fs::write(&b, "x = 2\n").unwrap();
        assert!(adopt(&dir, "Walls", &b).is_err());
    }

    #[test]
    fn refresh_reads_the_file_when_it_differs() {
        let dir = tempdir("refresh");
        let text = "# in: n: int = 1\nx = 1\n";
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("scripts/walls.py"), text).unwrap();
        let (s, r) = script_at("walls", "scripts/walls.py", text);
        assert_eq!(refresh(&s, Some(&r), &dir).unwrap(), None);

        std::fs::write(dir.join("scripts/walls.py"), "x = 2\n").unwrap();
        assert_eq!(
            refresh(&s, Some(&r), &dir).unwrap().as_deref(),
            Some("x = 2\n")
        );
    }

    #[test]
    fn missing_file_falls_back_and_unreadable_is_loud() {
        let dir = tempdir("missing");
        let (s, r) = script_at("gone", "scripts/gone.py", "x = 1\n");
        assert_eq!(refresh(&s, Some(&r), &dir).unwrap(), None);

        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("scripts/locked.py"), "x = 1\n").unwrap();
        let (locked, lr) = script_at("locked", "scripts/locked.py", "x = 1\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                dir.join("scripts/locked.py"),
                std::fs::Permissions::from_mode(0o000),
            )
            .unwrap();
        }
        assert!(refresh(&locked, Some(&lr), &dir).is_err());
    }

    #[test]
    fn write_through_creates_the_file_and_becomes_quiet() {
        let dir = tempdir("writethrough");
        let (s, r) = script_at("new", "scripts/new.py", "x = 3\n");
        write_through(&s, Some(&r), &dir).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("scripts/new.py")).unwrap(),
            "x = 3\n"
        );
        assert_eq!(refresh(&s, Some(&r), &dir).unwrap(), None);
    }

    #[test]
    fn digest_covers_helpers_and_notices_changes() {
        let dir = tempdir("digest");
        assert_eq!(digest(&dir), None);

        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("scripts/walls.py"), "x = 1\n").unwrap();
        let first = digest(&dir).unwrap();
        assert_eq!(digest(&dir).unwrap(), first, "quiet when nothing changed");

        std::fs::write(dir.join("scripts/helpers.py"), "def n():\n    return 1\n").unwrap();
        assert_ne!(
            digest(&dir).unwrap(),
            first,
            "a new helper changes the digest"
        );
    }
}
