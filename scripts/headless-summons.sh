#!/usr/bin/env bash
# Summons SAM many times without touching the desktop (SPIKE-D): a private
# session bus that can start no services, a headless mutter on its own
# Wayland display, and an SAM instance on both. Prints the instance's memory
# every 50 summons.
#
#   scripts/headless-summons.sh [BINARY] [SUMMONS]
#
#   BINARY   defaults to target/release/sam
#   SUMMONS  defaults to 200
#   PHRASES='a|b|c'     what each summon types, in turn
#   SAM_ENV='K=V …'     extra environment for SAM only
#
# The private bus has no LocalSearch, so file searches fail (and say so in
# the log). Nothing here takes Luis's keyboard focus.
set -euo pipefail

if [[ "${1:-}" == "--inner" ]]; then
  sam=$2; n=$3; display=$4; logs=$5
  mutter --headless --virtual-monitor 1536x864 --wayland --no-x11 \
    --wayland-display "$display" >"$logs/mutter.log" 2>&1 &
  mutter_pid=$!
  trap 'kill $mutter_pid 2>/dev/null || true' EXIT
  for _ in $(seq 50); do [[ -S "$XDG_RUNTIME_DIR/$display" ]] && break; sleep 0.1; done
  [[ -S "$XDG_RUNTIME_DIR/$display" ]] || { echo "the compositor did not start" >&2; exit 1; }

  # shellcheck disable=SC2086
  env ${SAM_ENV:-} WAYLAND_DISPLAY="$display" GDK_BACKEND=wayland NO_AT_BRIDGE=1 \
    "$sam" --gapplication-service >"$logs/sam.log" 2>&1 &
  sam_pid=$!
  for _ in $(seq 50); do
    gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
      --method org.freedesktop.DBus.NameHasOwner io.github.luishowin.Sam 2>/dev/null \
      | grep -q true && break
    sleep 0.1
  done
  sleep 1

  mem() {
    printf '%-12s' "$1"
    awk '/^(Rss|Pss|Anonymous):/ { printf "%s %6d kB  ", $1, $2 }' "/proc/$sam_pid/smaps_rollup"
    awk '/^Threads/ { print "threads " $2 }' "/proc/$sam_pid/status"
  }
  phrases=("chrome" "photodesk" "find invoice" "17 * 42" "downloads" "turn off bluetooth"
           "screenshot" "sett" "vsc" "find the PDF about invoices" "calculator" "~/Downloads/")
  if [[ -n "${PHRASES:-}" ]]; then IFS='|' read -r -a phrases <<< "$PHRASES"; fi

  mem "start"
  for i in $(seq 1 "$n"); do
    text=${phrases[$(( (i - 1) % ${#phrases[@]} ))]}
    gapplication action io.github.luishowin.Sam show-text "'$text'"
    sleep 0.25
    gapplication action io.github.luishowin.Sam hide
    sleep 0.05
    if (( i % 50 == 0 )); then mem "after $i"; fi
  done
  sleep 2
  mem "settled"
  gapplication action io.github.luishowin.Sam quit
  sleep 0.5
  exit 0
fi

cd "$(dirname "$0")/.."
binary=$(realpath "${1:-target/release/sam}")
summons=${2:-200}
[[ -x "$binary" ]] || { echo "no binary at $binary (cargo build --release)" >&2; exit 2; }

# Short paths: a Unix socket name has to fit in 108 bytes.
work=$(mktemp -d "$XDG_RUNTIME_DIR/sam-headless.XXXXXX")
trap 'rm -rf "$work"' EXIT
mkdir "$work/services"
cat >"$work/bus.conf" <<EOF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <keep_umask/>
  <listen>unix:dir=$work</listen>
  <auth>EXTERNAL</auth>
  <servicedir>$work/services</servicedir>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF

display="sam-headless-$$"
status=0
dbus-run-session --config-file="$work/bus.conf" -- \
  "$0" --inner "$binary" "$summons" "$display" "$work" || status=$?
echo "-- the end of SAM's log:"
tail -n 4 "$work/sam.log" 2>/dev/null || true
exit "$status"
