# Maintainer: nnthnn <todo@example.com>
pkgname=hyprview-git
pkgver=r0
pkgrel=1
pkgdesc='Lightweight workspace overview for Hyprland and Sway with live window thumbnails'
arch=('x86_64')
url='https://github.com/nnthnn/hyprview'
license=('MIT')
depends=('wayland' 'cairo' 'pango' 'hyprland>=0.55')
makedepends=('git' 'rust' 'cargo')
provides=('hyprview')
conflicts=('hyprview')
source=("git+${url}.git")
sha256sums=('SKIP')

pkgver() {
    cd hyprview
    printf "r%s.%s" "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"
}

build() {
    cd hyprview
    cargo build --release --locked
}

package() {
    cd hyprview
    install -Dm755 target/release/hyprview "$pkgdir/usr/bin/hyprview"
    install -Dm644 README.md "$pkgdir/usr/share/doc/$pkgname/README.md"
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
