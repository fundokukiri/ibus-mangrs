#!/bin/sh
# Build release packages for ibus-mangrs into dist/:
#   .pkg.tar.zst (Arch, via makepkg), .deb (hand-rolled, no dpkg needed),
#   .rpm (requires rpmbuild; skipped with a notice when it is missing).
#
#   ./scripts/dist.sh
# Re-run after every version bump. The binary must be built first:
#   cargo build --release --features gui
set -eu

here=$(cd "$(dirname "$0")/.." && pwd)
cd "$here"
out="$here/dist"
mkdir -p "$out"

version=$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)
pkgname=ibus-mangrs
bin=target/release/ibus-engine-ibus-mangrs
[ -x "$bin" ] || {
    echo "missing $bin — build first: cargo build --release --features gui" >&2
    exit 1
}

# The shared file layout every package ships.
stage() {
    d=$1
    install -Dm755 "$bin" "$d/usr/lib/ibus-mangrs/ibus-engine-ibus-mangrs"
    install -Dm644 data/ibus-mangrs.xml "$d/usr/share/ibus/component/ibus-mangrs.xml"
    install -Dm644 data/vn-words.txt "$d/usr/share/ibus-mangrs/vn-words.txt"
    install -Dm644 data/80-uinput.rules "$d/usr/lib/udev/rules.d/80-uinput.rules"
    install -Dm644 LICENSE "$d/usr/share/licenses/$pkgname/LICENSE"
    install -Dm644 README.md "$d/usr/share/doc/$pkgname/README.md"
    install -Dm644 CHANGELOG.md "$d/usr/share/doc/$pkgname/CHANGELOG.md"
}

# --- Arch: source tarball + makepkg -----------------------------------------
# PKGBUILD expects the source tarball as ibus-ibus-mangrs-<version>.tar.gz
# with the repo files at its root.
mkdir -p packaging/arch
cp PKGBUILD packaging/arch/
git archive --format=tar.gz -o "packaging/arch/ibus-ibus-mangrs-$version.tar.gz" HEAD
( cd packaging/arch
  # -d: the maintainer machine may use rustup instead of the `rust` package;
  # the dependency declarations stay in the PKGBUILD for end users.
  makepkg -f --skipinteg -d >/dev/null
  mv ./*.pkg.tar.zst "$out/" )
echo "built: dist/ibus-ibus-mangrs-$version-*.pkg.tar.zst"

# --- deb: hand-rolled ar archive (control.tar.gz + data.tar.zst) ------------
debstage=$(mktemp -d)
trap 'rm -rf "$debstage"' EXIT
stage "$debstage/data"
mkdir -p "$debstage/control/DEBIAN"
cat > "$debstage/control/DEBIAN/control" <<EOF
Package: $pkgname
Version: $version
Section: utils
Priority: optional
Architecture: amd64
Depends: ibus, libgtk-4-1
Maintainer: fundoku <https://github.com/fundokukiri>
Homepage: https://github.com/fundokukiri/ibus-mangrs
Description: Vietnamese input engine for IBus built on bamboo-core
 Telex, VNI, VIQR and Microsoft layouts, eight display modes including
 real-key (uinput) modes, macros, a custom dictionary, per-app rules and
 a GTK4 settings window.
EOF
cat > "$debstage/control/DEBIAN/postinst" <<'EOF'
#!/bin/sh
udevadm control --reload-rules 2>/dev/null || true
udevadm trigger /dev/uinput 2>/dev/null || true
echo "ibus-mangrs installed. Run: ibus restart, then add the input source."
echo "Backspace repair needs /dev/uinput: sudo usermod -aG input $USER (re-login)."
EOF
chmod 755 "$debstage/control/DEBIAN/postinst"
( cd "$debstage/control" && tar -czf control.tar.gz DEBIAN )
( cd "$debstage/data" && tar --zstd -cf data.tar.zst usr )
# ar member order matters: debian-binary, then control, then data.
deb="$out/${pkgname}_${version}_amd64.deb"
ar_dir=$(mktemp -d)
echo "2.0" > "$ar_dir/debian-binary"
( cd "$debstage/control" && tar -czf "$ar_dir/control.tar.gz" DEBIAN )
( cd "$debstage/data" && tar -cJf "$ar_dir/data.tar.xz" usr )
( cd "$ar_dir" && ar -r "$deb" debian-binary control.tar.gz data.tar.xz )
rm -rf "$ar_dir"
echo "built: dist/${pkgname}_${version}_amd64.deb"

# --- rpm: needs rpmbuild ----------------------------------------------------
if command -v rpmbuild >/dev/null; then
    rpmdir=$(mktemp -d)
    mkdir -p "$rpmdir/BUILD" "$rpmdir/RPMS" "$rpmdir/SOURCES" "$rpmdir/SPECS" "$rpmdir/SRPMS"
    tar -czf "$rpmdir/SOURCES/${pkgname}-$version.tar.gz" \
        --transform "s,^,${pkgname}-$version/," \
        src data scripts LICENSE README.md CHANGELOG.md Cargo.toml Cargo.lock
    cat > "$rpmdir/SPECS/$pkgname.spec" <<EOF
Name:           $pkgname
Version:        $version
Release:        1%{?dist}
Summary:        Vietnamese input engine for IBus built on bamboo-core
License:        MIT
URL:            https://github.com/fundokukiri/ibus-mangrs
Source0:        %{name}-%{version}.tar.gz
BuildRequires:  rust cargo
Requires:       ibus, gtk4

%description
Vietnamese input engine for IBus: Telex, VNI, VIQR and Microsoft layouts,
eight display modes including real-key (uinput) modes, macros, a custom
dictionary, per-app rules and a GTK4 settings window.

%prep
%setup -q

%build
cargo build --release --features gui

%install
install -Dm755 target/release/ibus-engine-ibus-mangrs %{buildroot}/usr/lib/ibus-mangrs/ibus-engine-ibus-mangrs
install -Dm644 data/ibus-mangrs.xml %{buildroot}/usr/share/ibus/component/ibus-mangrs.xml
install -Dm644 data/vn-words.txt %{buildroot}/usr/share/ibus-mangrs/vn-words.txt
install -Dm644 data/80-uinput.rules %{buildroot}/usr/lib/udev/rules.d/80-uinput.rules
install -Dm644 LICENSE %{buildroot}/usr/share/licenses/%{name}/LICENSE

%post
udevadm control --reload-rules 2>/dev/null || true
udevadm trigger /dev/uinput 2>/dev/null || true

%files
/usr/lib/ibus-mangrs/ibus-engine-ibus-mangrs
/usr/share/ibus/component/ibus-mangrs.xml
/usr/share/ibus-mangrs/vn-words.txt
/usr/lib/udev/rules.d/80-uinput.rules
/usr/share/licenses/%{name}/LICENSE
EOF
    rpmbuild -bb --define "_topdir $rpmdir" "$rpmdir/SPECS/$pkgname.spec" >/dev/null
    mv "$rpmdir"/RPMS/x86_64/*.rpm "$out/"
    rm -rf "$rpmdir"
    echo "built: dist/${pkgname}-$version-1.*.rpm"
else
    echo "NOTE: rpmbuild not installed — .rpm skipped. Install it with:" \
         "sudo pacman -S rpm-org-tools   (then re-run ./scripts/dist.sh)" >&2
fi

echo
ls -la "$out"
