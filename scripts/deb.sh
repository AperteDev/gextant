#!/bin/sh
# Package a staged tree (`make stage`, the tarball's content) as a .deb. Run it on the Ubuntu
# release the package is for: dpkg-shlibdeps takes the dependencies from the libraries the
# binary links to, as that release names them.
#
#   scripts/deb.sh <staged tree> <version> <out dir>
set -eu

stage=$1 version=$2 out=$3
arch=$(dpkg --print-architecture)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cp -a "$stage" "$work/root"
# dpkg-shlibdeps reads a source package's control file from the current directory.
mkdir -p "$work/src/debian"
printf 'Source: gextant\n\nPackage: gextant\nArchitecture: any\n' > "$work/src/debian/control"
depends=$(cd "$work/src" && dpkg-shlibdeps -O "$work/root/usr/bin/gextant" |
  sed -n 's/^shlibs:Depends=//p')

mkdir -p "$work/root/DEBIAN"
cat > "$work/root/DEBIAN/control" <<EOF
Package: gextant
Version: $version
Architecture: $arch
Maintainer: SelfRef <git@selfref.dev>
Installed-Size: $(du -sk "$work/root" | cut -f1)
Depends: $depends, ca-certificates
Recommends: gnome-keyring
Section: gnome
Priority: optional
Homepage: https://github.com/AperteDev/gextant
Description: Updates GNOME Shell extensions from their repositories
 Gextant lists the GNOME Shell extensions installed on this computer, finds
 each one's repository (GitHub or GitLab) and shows the newest version there
 and on extensions.gnome.org beside the one you have.
EOF

mkdir -p "$out"
dpkg-deb --root-owner-group --build "$work/root" "$out/gextant_${version}_$arch.deb"
