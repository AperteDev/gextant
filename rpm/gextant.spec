# Gextant for Fedora, built on COPR from .copr/Makefile (the crates vendored next to the
# sources; rust-libsecret is not in Fedora yet). scripts/release.py sets the version and adds
# the changelog entry.
%bcond check 1

Name:           gextant
Version:        0.1.0
Release:        1%{?dist}
Summary:        Updates GNOME Shell extensions from their repositories

SourceLicense:  GPL-3.0-or-later
# The app, and the crates linked into it (%%cargo_license_summary; LICENSE.dependencies lists
# them by crate). Check it again when the dependencies change.
License:        GPL-3.0-or-later AND Apache-2.0 AND BSD-3-Clause AND CDLA-Permissive-2.0 AND ISC AND MIT AND (0BSD OR MIT OR Apache-2.0) AND (Apache-2.0 OR ISC OR MIT) AND (Apache-2.0 OR MIT) AND (Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT) AND (MIT OR Zlib OR Apache-2.0) AND (Unlicense OR MIT)
URL:            https://github.com/AperteDev/gextant
Source0:        %{url}/archive/v%{version}/%{name}-%{version}.tar.gz
# cargo vendor --locked, as .copr/Makefile makes it.
Source1:        %{name}-%{version}-vendor.tar.xz

BuildRequires:  cargo-rpm-macros >= 25
BuildRequires:  gcc
BuildRequires:  make
BuildRequires:  blueprint-compiler
BuildRequires:  pkgconfig(gtk4)
BuildRequires:  pkgconfig(libadwaita-1) >= 1.7
BuildRequires:  pkgconfig(libsecret-1)
BuildRequires:  glib2-devel
BuildRequires:  desktop-file-utils
BuildRequires:  appstream

# The binary links to the library; its version is for the widgets the views use.
Requires:       libadwaita%{?_isa} >= 1.7
Requires:       ca-certificates
Suggests:       gnome-keyring

%description
Gextant checks your GNOME Shell extensions against the repositories they
come from, so a fix or a release reaches you without waiting for the review
on extensions.gnome.org. It lists the extensions installed on this computer,
finds each one's repository (GitHub or GitLab) and shows the newest version
there and on extensions.gnome.org beside the one you have.

%prep
%autosetup -p1 -a1
%cargo_prep -v vendor

%build
%cargo_build
%{cargo_license_summary}
%{cargo_license} > LICENSE.dependencies
%cargo_vendor_manifest

%install
make install-data PREFIX=%{_prefix} DESTDIR=%{buildroot}

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/dev.aperte.Gextant.desktop
appstreamcli validate --no-net %{buildroot}%{_metainfodir}/dev.aperte.Gextant.metainfo.xml
%if %{with check}
%cargo_test
%endif

%files
%license LICENSE LICENSE.dependencies cargo-vendor.txt
%doc README.md docs/CHANGELOG.md
%{_bindir}/gextant
%{_datadir}/applications/dev.aperte.Gextant.desktop
%{_metainfodir}/dev.aperte.Gextant.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/dev.aperte.Gextant.svg
%{_datadir}/icons/hicolor/symbolic/apps/dev.aperte.Gextant-symbolic.svg

%changelog
* Thu Nov 05 2026 SelfRef <git@selfref.dev> - 0.1.0-1
- See CHANGELOG.md
