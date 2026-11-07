#!/usr/bin/env bash
# Install the local Needle sidecar into ~/.local as a user service.
#
# This does not install or replace the `sam` binary; run
# scripts/install-user.sh for that. The sidecar is only useful once the new
# Sam build is running.
#
#   scripts/install-needle-dev.sh
#   scripts/install-needle-dev.sh --uninstall
#
# NEEDLE_RESOURCES points at the directory containing sam-needle.cact.
# The runtime is ~/.local/share/sam/needle/venv, built with
# `pip install cactus-compute==4.0.1`; override it with SAM_NEEDLE_PYTHON.
set -euo pipefail
cd "$(dirname "$0")/.."

resources="${NEEDLE_RESOURCES:-$HOME/Desktop/needle fine tune resources}"
python="${SAM_NEEDLE_PYTHON:-$HOME/.local/share/sam/needle/venv/bin/python}"
weights="$resources/sam-needle.cact"
libexec="$HOME/.local/libexec/sam-needle"
data="$HOME/.local/share/sam/needle"
unit="$HOME/.config/systemd/user/sam-needle.service"
service="sam-needle.service"

if [[ "${1:-}" == "--uninstall" ]]; then
  systemctl --user disable --now "$service" 2>/dev/null || true
  rm -f "$unit" "$libexec/sam_needle_sidecar.py" "$data/sam-needle.cact"
  systemctl --user daemon-reload
  echo "removed the Needle sidecar"
  exit 0
fi

if [[ ! -x "$python" ]]; then
  echo "needle: no pinned Python at $python" >&2
  echo "set NEEDLE_RESOURCES to the fine-tune resources directory" >&2
  exit 2
fi
if [[ ! -f "$weights" ]]; then
  echo "needle: no model at $weights" >&2
  exit 2
fi

install -Dm755 packaging/needle/sam_needle_sidecar.py \
  "$libexec/sam_needle_sidecar.py"
install -Dm644 "$weights" "$data/sam-needle.cact"
mkdir -p "$(dirname "$unit")"
sed -e "s|@python@|$python|g" \
  -e "s|@script@|$libexec/sam_needle_sidecar.py|g" \
  -e "s|@weights@|$data/sam-needle.cact|g" \
  packaging/needle/sam-needle.service > "$unit"

systemctl --user daemon-reload
systemctl --user enable "$service"
systemctl --user restart "$service"
sleep 1
systemctl --user --no-pager status "$service" | head -12
echo "installed the Needle sidecar; check it with: sam doctor"
