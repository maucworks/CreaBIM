# Releasing CreaBIM

Versions follow semantic versioning (`MAJOR.MINOR.PATCH`). Each release is also named by the day it was tagged, so the release and its files carry both:
- release: **CreaBIM 1.0.0 · 2026-11-09**
- files: `CreaBIM-1.0.0-2026-11-09-windows-x64.zip`, `CreaBIM-1.0.0-2026-11-09-linux-x64.tar.gz`, `CreaBIM-1.0.0-2026-11-09-linux-x64.deb`, `CreaBIM-1.0.0-2026-11-09-linux-x64.rpm`, `CreaBIM-1.0.0-2026-11-09-macos-universal.dmg`, `SHA256SUMS.txt`

The date is ISO (year-month-day), so it reads the same in every country and sorts in order.

Steps:
1. Set `version` in the workspace `Cargo.toml`. Run the checks:
   - `cargo fmt --all --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace`

   Then merge to `main`.
2. Tag the merge commit and push the tag:
   - `git tag v1.0.0`
   - `git push origin v1.0.0`
3. The **Release** workflow builds Windows, Linux (including Ubuntu `.deb` and Fedora and RHEL 10 `.rpm` packages), and a universal macOS `.dmg`, writes the checksums, and opens a **draft** release.
4. Read the draft, add highlights, then publish it.

Download links always point at `https://github.com/chopinregis/CreaBIM/releases/latest`.
