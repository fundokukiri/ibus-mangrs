# PKGBUILD for MăngRS (Arch Linux)
# Build: makepkg -si

pkgname=ibus-ibus-mangrs
pkgver=0.1.0
pkgrel=1
pkgdesc="Vietnamese input engine for IBus, built on bamboo-core (Rust)"
arch=("x86_64")
url="https://github.com/fundokukiri/ibus-mangrs"
license=("MIT")
depends=("ibus" "gtk4")
makedepends=("rust")
source=("$pkgname-$pkgver.tar.gz")
sha256sums=("SKIP")

build() {
    cd "$srcdir"
    cargo build --release --features gui
}

package() {
    cd "$srcdir"
    install -Dm755 target/release/ibus-engine-ibus-mangrs \
        "$pkgdir/usr/lib/ibus-mangrs/ibus-engine-ibus-mangrs"
    install -Dm644 data/ibus-mangrs.xml \
        "$pkgdir/usr/share/ibus/component/ibus-mangrs.xml"
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
