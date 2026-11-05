# Sam's RPM spec. Builds offline from a source tarball and a vendor tarball,
# both made by packaging/rpm/make-vendor-tarball.sh (see README.md there).
# The version is Cargo.toml's; %prep stops if this file disagrees with it.

%global debug_package %{nil}
%global app_id io.github.luishowin.Sam

Name:           sam
Version:        0.1.0
Release:        1%{?dist}
Summary:        Search execution engine for this computer
License:        MIT
URL:            https://github.com/luishowin/sam
Source0:        %{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.xz

BuildRequires:  cargo >= 1.92
BuildRequires:  rust >= 1.92
BuildRequires:  gcc
BuildRequires:  pkgconfig(gtk4) >= 4.22
BuildRequires:  pkgconfig(libadwaita-1) >= 1.9
BuildRequires:  pkgconfig(glib-2.0) >= 2.84
BuildRequires:  libtinysparql
BuildRequires:  systemd-rpm-macros
BuildRequires:  desktop-file-utils

Requires:       glib2
Requires:       wireplumber
Recommends:     localsearch
Recommends:     ptyxis
Recommends:     bluez
Recommends:     NetworkManager
Recommends:     power-profiles-daemon
Recommends:     pipewire-utils
Recommends:     libnotify

%description
Sam is a deterministic search execution engine for this computer. Super+Space
opens a small panel; type, and it finds applications, files, folders, settings,
live system state and hardware information. It performs only fixed, reviewed
actions: open, launch, reveal, copy and a small set of desktop switches.

There is no language model in the runtime, no network egress and no personal
memory. Text is matched against a versioned registry and read-only local
sources.

After installing, run `sam setup` once as your user to bind Super+Space and
start the service; `sam setup --undo` reverses it.

%prep
%autosetup -p1
tar -xf %{SOURCE1}
mkdir -p .cargo
cat vendor-config.toml >> .cargo/config.toml
grep -q '^version = "%{version}"$' Cargo.toml

%build
cargo build --release --locked --offline -p sam

%check
cargo test --release --locked --offline -p sam-core -p sam-linux

%install
install -Dm0755 target/release/sam %{buildroot}%{_bindir}/sam
strip %{buildroot}%{_bindir}/sam
install -Dm0644 data/%{app_id}.desktop %{buildroot}%{_datadir}/applications/%{app_id}.desktop
install -d %{buildroot}%{_datadir}/dbus-1/services %{buildroot}%{_userunitdir}
sed 's|@bindir@|%{_bindir}|g' data/dbus/%{app_id}.service \
    > %{buildroot}%{_datadir}/dbus-1/services/%{app_id}.service
sed 's|@bindir@|%{_bindir}|g' data/systemd/%{app_id}.service \
    > %{buildroot}%{_userunitdir}/%{app_id}.service
chmod 0644 %{buildroot}%{_datadir}/dbus-1/services/%{app_id}.service \
    %{buildroot}%{_userunitdir}/%{app_id}.service
install -Dm0644 data/icons/hicolor/512x512/apps/%{app_id}.png \
    %{buildroot}%{_datadir}/icons/hicolor/512x512/apps/%{app_id}.png
install -Dm0644 data/icons/hicolor/scalable/apps/%{app_id}.svg \
    %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/%{app_id}.svg
install -Dm0644 data/sounds/listening.wav \
    %{buildroot}%{_datadir}/sam/sounds/listening.wav
desktop-file-validate %{buildroot}%{_datadir}/applications/%{app_id}.desktop

%files
%license LICENSE
%doc README.md docs/fedora-prototype.md docs/migration-from-ida.md
%{_bindir}/sam
%{_datadir}/applications/%{app_id}.desktop
%{_datadir}/dbus-1/services/%{app_id}.service
%{_userunitdir}/%{app_id}.service
%{_datadir}/icons/hicolor/512x512/apps/%{app_id}.png
%{_datadir}/icons/hicolor/scalable/apps/%{app_id}.svg
%{_datadir}/sam/sounds/listening.wav

%changelog
* Thu Nov 05 2026 Luis Howin Maina <howin329@gmail.com> - 0.1.0-1
- First Sam build: deterministic search over apps, files, folders, settings,
  live system state, hardware and local documentation.
- Fixed quick actions for session, sound, media, Bluetooth, Wi-Fi, airplane
  mode, night light, dark style, focus mode, power profile and terminal.
- Local timers and alarms through systemd user timers.
- Replaces Ida on this machine; no AI mode, no network egress, no memory.
