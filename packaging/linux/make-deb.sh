#!/usr/bin/env bash
set -euo pipefail
umask 022

if [[ $# -ne 4 ]]; then
    echo "Usage: $0 VERSION DATE TARGET_DIR OUTPUT.deb" >&2
    exit 2
fi

version=$1
date=$2
target_dir=$3
output=$4

if [[ ! $version =~ ^[0-9][A-Za-z0-9.+:~\-]*$ ]]; then
    echo "Invalid Debian package version: $version" >&2
    exit 2
fi
if [[ ! $date =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
    echo "Invalid release date: $date" >&2
    exit 2
fi

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

chmod 0755 "$stage"
install -d "$stage/DEBIAN"
install -d "$stage/usr/bin"
install -d "$stage/usr/share/applications"
install -d "$stage/usr/share/icons/hicolor/512x512/apps"
install -d "$stage/usr/share/doc/creabim"

install -m 0755 "$target_dir/creabim-app" "$stage/usr/bin/creabim"
install -m 0755 "$target_dir/creabim" "$stage/usr/bin/creabim-cli"
install -m 0644 crates/creabim-app/assets/logo/png/creabim-app-icon-512.png \
    "$stage/usr/share/icons/hicolor/512x512/apps/creabim.png"
install -m 0644 LICENSE "$stage/usr/share/doc/creabim/copyright"
install -m 0644 README.md "$stage/usr/share/doc/creabim/README.md"
install -m 0644 docs/RELEASE_README.txt "$stage/usr/share/doc/creabim/README.txt"

cat > "$stage/DEBIAN/control" <<CONTROL
Package: creabim
Version: $version
Section: graphics
Priority: optional
Architecture: amd64
Maintainer: Regis Benoit Brice Nde Tene <chopinregis@users.noreply.github.com>
Depends: libc6, libgtk-3-0 | libgtk-3-0t64, libwebkit2gtk-4.1-0, libxkbcommon-x11-0
Homepage: https://github.com/chopinregis/CreaBIM
Description: free, open-source BIM modeller
 Model a building in 3D and produce its plans, sections, elevations,
 sheets, schedules, quantities, and IFC from the same model.
CONTROL
chmod 0644 "$stage/DEBIAN/control"

cat > "$stage/usr/share/applications/creabim.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=CreaBIM
Comment=Free, open-source BIM modeller
Exec=creabim
TryExec=creabim
Icon=creabim
Terminal=false
Categories=Graphics;3DGraphics;Engineering;
DESKTOP
chmod 0644 "$stage/usr/share/applications/creabim.desktop"

mkdir -p "$(dirname "$output")"
dpkg-deb --build --root-owner-group "$stage" "$output"
echo "Created $output for CreaBIM $version (release date $date)"
