#!/usr/bin/env bash
set -euo pipefail
umask 022

if [[ $# -ne 4 ]]; then
    echo "Usage: $0 VERSION DATE TARGET_DIR OUTPUT.rpm" >&2
    exit 2
fi

version=$1
date=$2
target_dir=$3
output=$4

if [[ ! $version =~ ^[0-9][A-Za-z0-9.+~-]*$ ]]; then
    echo "Invalid RPM package version: $version" >&2
    exit 2
fi
if [[ ! $date =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
    echo "Invalid release date: $date" >&2
    exit 2
fi

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root"

build=$(mktemp -d)
trap 'rm -rf "$build"' EXIT

topdir="$build/rpmbuild"
install -d "$topdir"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}

stage="$topdir/SOURCES/root"
install -d "$stage/usr/bin" "$stage/usr/share/applications" \
    "$stage/usr/share/icons/hicolor/512x512/apps" "$stage/usr/share/doc/creabim"

install -m 0755 "$target_dir/creabim-app" "$stage/usr/bin/creabim"
install -m 0755 "$target_dir/creabim" "$stage/usr/bin/creabim-cli"
install -m 0644 crates/creabim-app/assets/logo/png/creabim-app-icon-512.png \
    "$stage/usr/share/icons/hicolor/512x512/apps/creabim.png"
install -m 0644 LICENSE "$stage/usr/share/doc/creabim/copyright"
install -m 0644 README.md "$stage/usr/share/doc/creabim/README.md"
install -m 0644 docs/RELEASE_README.txt "$stage/usr/share/doc/creabim/README.txt"

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

rpm_separator='~'
rpm_version=${version//-/$rpm_separator}
cat > "$topdir/SPECS/creabim.spec" <<SPEC
Name:           creabim
Version:        $rpm_version
Release:        1%{?dist}
Summary:        Free, open-source BIM modeller
License:        GPL-3.0-or-later
URL:            https://github.com/chopinregis/CreaBIM
BuildArch:      x86_64
Requires:       glibc >= 2.35
Requires:       gtk3
Requires:       webkit2gtk4.1
Requires:       libxkbcommon-x11

%description
Model a building in 3D and produce its plans, sections, elevations,
sheets, schedules, quantities, and IFC from the same model.

%prep

%build

%install
mkdir -p "%{buildroot}"
cp -a "%{_sourcedir}/root/." "%{buildroot}/"

%files
%{_bindir}/creabim
%{_bindir}/creabim-cli
%{_datadir}/applications/creabim.desktop
%{_datadir}/icons/hicolor/512x512/apps/creabim.png
%dir %{_datadir}/doc/creabim
%license %{_datadir}/doc/creabim/copyright
%{_datadir}/doc/creabim/README.md
%{_datadir}/doc/creabim/README.txt
SPEC

mkdir -p "$(dirname "$output")"
rpmbuild -bb --define "_topdir $topdir" "$topdir/SPECS/creabim.spec"
rpm_file=$(find "$topdir/RPMS" -type f -name '*.rpm' -print -quit)
if [[ -z $rpm_file ]]; then
    echo "rpmbuild completed without producing an RPM" >&2
    exit 1
fi
cp "$rpm_file" "$output"
echo "Created $output for CreaBIM $version (release date $date)"
