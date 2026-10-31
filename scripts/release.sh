#!/bin/sh
# Full release pipeline, one command:
#
#   ./scripts/release.sh
#
# 1. builds the release binary (cargo, with the GTK4 GUI)
# 2. produces the three packages into dist/ via scripts/dist.sh:
#      - ibus-ibus-mangrs-<version>-1-x86_64.pkg.tar.zst  (Arch)
#      - ibus-mangrs_<version>_amd64.deb                  (Debian/Ubuntu)
#      - ibus-mangrs-<version>-1.x86_64.rpm               (Fedora/openSUSE, needs rpmbuild)
#      - SHA256SUMS (self-verified at the end)
# 3. prints the upload checklist for the GitHub release.
#
# The packages stay in dist/ (gitignored) — only this tool is in the repo.
set -eu

here=$(cd "$(dirname "$0")/.." && pwd)
cd "$here"

version=$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)
echo "== ibus-mangrs $version =="
echo

echo ">> [1/3] building the release binary..."
cargo build --release --features gui

echo
echo ">> [2/3] building packages..."
./scripts/dist.sh

echo
echo ">> [3/3] verifying checksums..."
( cd dist && sha256sum -c SHA256SUMS )

echo
echo "== done. upload checklist =="
echo "  1. merge the release PR into master (or push master)"
echo "  2. GitHub -> Releases -> Draft a new release, tag: v$version"
echo "  3. attach the files below from dist/ (not the -debug package):"
ls dist/ | grep -v debug | sed 's/^/       dist\//'
echo "  4. paste the description from the [$version] section of CHANGELOG.md"
echo "  5. publish"
