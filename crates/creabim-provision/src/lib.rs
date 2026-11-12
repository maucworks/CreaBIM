//! Provisioning: a pinned python-build-standalone distribution,
//! checksummed and extracted. Loud errors, never a silent fallback
//! to a system Python (plan trap 9).
//!
//! PyO3-independent on purpose: the interpreter must exist *before*
//! anything linking PyO3 builds, so this crate links no Python.
//! Build-time setup is therefore two steps (see crate docs):
//! `cargo run -p creabim-provision` first, then build with
//! `PYO3_PYTHON=<home>/bin/python3.13`.

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProvisionError {
    #[error("scripting is not supported on this platform")]
    UnsupportedPlatform,
    #[error("download failed: {0}")]
    Download(String),
    #[error("checksum failure: {0}")]
    Checksum(String),
    #[error("io: {0}")]
    Io(String),
}

/// Pinned CPython version (3.13: mature wheel ecosystem; 3.14 later).
pub const PINNED_PYTHON: &str = "3.13.18";
/// Pinned python-build-standalone release tag.
pub const PINNED_TAG: &str = "20261110";

const BASE_URL: &str = "https://github.com/astral-sh/python-build-standalone/releases/download";

/// Asset file name per supported platform (keep CI's ubuntu + windows
/// in sync when adding platforms).
pub fn asset_name() -> Result<&'static str, ProvisionError> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Ok("cpython-3.13.18+20261110-aarch64-apple-darwin-install_only.tar.gz")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Ok("cpython-3.13.18+20261110-x86_64-apple-darwin-install_only.tar.gz")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Ok("cpython-3.13.18+20261110-x86_64-unknown-linux-gnu-install_only.tar.gz")
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Ok("cpython-3.13.18+20261110-x86_64-pc-windows-msvc-install_only.tar.gz")
    } else {
        Err(ProvisionError::UnsupportedPlatform)
    }
}

/// Default cache dir; `CREABIM_PYTHON_HOME` overrides it.
/// The interpreter home is one level down (see [`home_under`]).
pub fn cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CREABIM_PYTHON_HOME") {
        return PathBuf::from(dir);
    }
    std::env::temp_dir().join(format!("creabim-python-{PINNED_PYTHON}-{PINNED_TAG}"))
}

fn marker(home: &Path, asset: &str) -> PathBuf {
    home.join(format!(".provisioned-ok-{asset}"))
}

fn download(url: &str) -> Result<Vec<u8>, ProvisionError> {
    let mut response = ureq::get(url)
        .call()
        .map_err(|e| ProvisionError::Download(format!("{url}: {e}")))?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| ProvisionError::Download(format!("{url}: {e}")))?;
    Ok(bytes)
}

fn expected_sha256(sums: &str, asset: &str) -> Result<String, ProvisionError> {
    sums.lines()
        .find_map(|line| {
            let (hash, name) = line.split_once(char::is_whitespace)?;
            (name.trim() == asset).then(|| hash.trim().to_string())
        })
        .ok_or_else(|| ProvisionError::Checksum(format!("{asset} not listed in SHA256SUMS")))
}

/// The `install_only` archives nest everything under `python/`;
/// this is the interpreter home for a cache root.
pub fn home_under(root: &Path) -> PathBuf {
    root.join("python")
}

/// Download (unless cached and marked), verify, extract.
/// Returns the home dir (the archive's top-level `python/` directory).
pub fn ensure_to(home: &Path) -> Result<PathBuf, ProvisionError> {
    let asset = asset_name()?;
    let nested = home_under(home);
    if nested.is_dir() && marker(&nested, asset).is_file() {
        return Ok(nested);
    }
    std::fs::create_dir_all(home).map_err(|e| ProvisionError::Io(format!("{home:?}: {e}")))?;

    let sums_url = format!("{BASE_URL}/{PINNED_TAG}/SHA256SUMS");
    let sums = String::from_utf8(download(&sums_url)?)
        .map_err(|e| ProvisionError::Checksum(format!("SHA256SUMS is not UTF-8: {e}")))?;
    let want = expected_sha256(&sums, asset)?;

    let asset_url = format!("{BASE_URL}/{PINNED_TAG}/{asset}");
    let bytes = download(&asset_url)?;
    let got = format!("{:x}", Sha256::digest(&bytes));
    if got != want {
        return Err(ProvisionError::Checksum(format!(
            "{asset}: got {got}, want {want}"
        )));
    }

    let gz = flate2::read::GzDecoder::new(&bytes[..]);
    let mut archive = tar::Archive::new(gz);
    archive
        .unpack(home)
        .map_err(|e| ProvisionError::Io(format!("extract {asset}: {e}")))?;
    std::fs::write(marker(&nested, asset), &want)
        .map_err(|e| ProvisionError::Io(format!("marker: {e}")))?;
    Ok(nested)
}

/// Download to the default cache dir (or its override).
pub fn ensure() -> Result<PathBuf, ProvisionError> {
    ensure_to(&cache_dir())
}
