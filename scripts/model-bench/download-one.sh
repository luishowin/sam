#!/usr/bin/env bash
# Download one GGUF into the benchmark cache.
# Usage: download-one.sh <repo> <file>
set -euo pipefail
repo="$1"
file="$2"
MODELS="${SAM_BENCH_MODELS:-$HOME/.cache/sam-bench/models}"
BASE="${SAM_BENCH_HF_BASE:-https://huggingface.co}"
mkdir -p "$MODELS"
out="$MODELS/$file"
if [[ -s "$out" ]]; then
  echo "cached $file"
  exit 0
fi
echo "fetch  $file"
curl -L --fail --retry 5 --retry-delay 5 -C - \
  -o "$out.part" "$BASE/$repo/resolve/main/$file"
mv "$out.part" "$out"
