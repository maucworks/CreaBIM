//! Provision the pinned CPython before anything links PyO3.
//!
//! Usage: `cargo run -p creabim-provision [-- <dir>]`, then build or
//! test with `PYO3_PYTHON=<home>/bin/python3.13` so PyO3 links the
//! bundled libpython instead of a system one.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(home) => {
            println!("{home:?}: provisioned");
            println!(
                "then: PYO3_PYTHON={}/bin/python3.13 cargo ...",
                home.display()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("provisioning failed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<PathBuf, creabim_provision::ProvisionError> {
    let dir = std::env::args().nth(1).map(PathBuf::from);
    match dir {
        Some(d) => creabim_provision::ensure_to(&d),
        None => creabim_provision::ensure(),
    }
}
