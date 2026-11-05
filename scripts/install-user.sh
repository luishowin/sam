#!/usr/bin/env bash
# A development install of SAM into ~/.local, then `sam setup` (Super+Space
# and the user service). The RPM installs the same files under /usr.
#
#   scripts/install-user.sh              build (release), install, set up
#   scripts/install-user.sh --uninstall  `sam setup --undo`, remove the files
set -euo pipefail
cd "$(dirname "$0")/.."

app_id=io.github.luishowin.Sam
bin_dir="$HOME/.local/bin"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
unit_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
files=(
  "$bin_dir/sam"
  "$data_dir/applications/$app_id.desktop"
  "$data_dir/dbus-1/services/$app_id.service"
  "$unit_dir/$app_id.service"
  "$data_dir/icons/hicolor/512x512/apps/$app_id.png"
  "$data_dir/sam/sounds/listening.wav"
)

# dbus-broker does not watch its service directories.
reload_bus() {
  busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus ReloadConfig >/dev/null
}

if [[ "${1:-}" == "--uninstall" ]]; then
  if [[ -x "$bin_dir/sam" ]]; then
    "$bin_dir/sam" setup --undo || true
  fi
  rm -f "${files[@]}"
  systemctl --user daemon-reload
  reload_bus
  echo "removed SAM from ~/.local"
  exit 0
fi

cargo build --release --locked

install -Dm755 target/release/sam "$bin_dir/sam"
sed "s|^Exec=sam|Exec=$bin_dir/sam|" "data/$app_id.desktop" \
  | install -Dm644 /dev/stdin "$data_dir/applications/$app_id.desktop"
sed "s|@bindir@|$bin_dir|g" "data/dbus/$app_id.service" \
  | install -Dm644 /dev/stdin "$data_dir/dbus-1/services/$app_id.service"
sed "s|@bindir@|$bin_dir|g" "data/systemd/$app_id.service" \
  | install -Dm644 /dev/stdin "$unit_dir/$app_id.service"
install -Dm644 "data/icons/hicolor/512x512/apps/$app_id.png" \
  "$data_dir/icons/hicolor/512x512/apps/$app_id.png"
install -Dm644 "data/icons/hicolor/scalable/apps/$app_id.svg" \
  "$data_dir/icons/hicolor/scalable/apps/$app_id.svg"
# Without an index, gtk-update-icon-cache refuses and GNOME keeps reading
# the stale cache (which hid the new icon); then rebuild the cache.
install -Dm644 data/icons/hicolor/index.theme "$data_dir/icons/hicolor/index.theme"
rm -f "$data_dir/icons/hicolor/icon-theme.cache"
gtk-update-icon-cache -f "$data_dir/icons/hicolor" >/dev/null
# The listening cue (§16): what `pw-play` hears on session start.
install -Dm644 data/sounds/listening.wav "$data_dir/sam/sounds/listening.wav"

reload_bus
# A running instance is the old binary; the unit restart below replaces it.
systemctl --user daemon-reload
"$bin_dir/sam" setup
systemctl --user restart "$app_id.service"
echo "installed SAM $("$bin_dir/sam" --version | cut -d' ' -f2) into ~/.local"
