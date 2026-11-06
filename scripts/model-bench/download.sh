#!/usr/bin/env bash
# Download the GGUF models used by the Tier 3 benchmark.
#
# Usage:
#   scripts/model-bench/download.sh [stage]
#
# stage: screen (default), quants, all
#
# Models are cached outside the repository so that `cargo clean` cannot delete
# them. Override with SAM_BENCH_MODELS.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MODELS="${SAM_BENCH_MODELS:-$HOME/.cache/sam-bench/models}"
STAGE="${1:-screen}"

mkdir -p "$MODELS"

# repo/file pairs
screen=(
  "unsloth/Qwen3-0.6B-GGUF Qwen3-0.6B-Q4_K_M.gguf"
  "unsloth/Qwen3-1.7B-GGUF Qwen3-1.7B-Q4_K_M.gguf"
  "unsloth/Qwen3-4B-GGUF Qwen3-4B-Q4_K_M.gguf"
  "unsloth/Qwen3.5-0.8B-GGUF Qwen3.5-0.8B-Q4_K_M.gguf"
  "unsloth/Qwen3.5-2B-GGUF Qwen3.5-2B-Q4_K_M.gguf"
  "unsloth/Qwen3.5-4B-GGUF Qwen3.5-4B-Q4_K_M.gguf"
  "unsloth/gemma-4-E2B-it-GGUF gemma-4-E2B-it-Q4_K_M.gguf"
  "unsloth/gemma-4-E4B-it-GGUF gemma-4-E4B-it-Q4_K_M.gguf"
)

# Extra quants for the staged sweep. These are fetched only when needed.
quants=(
  "unsloth/Qwen3-0.6B-GGUF Qwen3-0.6B-Q4_0.gguf"
  "unsloth/Qwen3-0.6B-GGUF Qwen3-0.6B-Q5_K_M.gguf"
  "unsloth/Qwen3-0.6B-GGUF Qwen3-0.6B-Q8_0.gguf"
  "unsloth/Qwen3-1.7B-GGUF Qwen3-1.7B-Q4_0.gguf"
  "unsloth/Qwen3-1.7B-GGUF Qwen3-1.7B-Q5_K_M.gguf"
  "unsloth/Qwen3-1.7B-GGUF Qwen3-1.7B-Q8_0.gguf"
  "unsloth/Qwen3-4B-GGUF Qwen3-4B-Q3_K_M.gguf"
  "unsloth/Qwen3-4B-GGUF Qwen3-4B-Q4_0.gguf"
  "unsloth/Qwen3-4B-GGUF Qwen3-4B-Q5_K_M.gguf"
  "unsloth/Qwen3-4B-GGUF Qwen3-4B-Q8_0.gguf"
  "unsloth/Qwen3.5-0.8B-GGUF Qwen3.5-0.8B-Q4_0.gguf"
  "unsloth/Qwen3.5-0.8B-GGUF Qwen3.5-0.8B-Q5_K_M.gguf"
  "unsloth/Qwen3.5-0.8B-GGUF Qwen3.5-0.8B-Q8_0.gguf"
  "unsloth/Qwen3.5-2B-GGUF Qwen3.5-2B-Q4_0.gguf"
  "unsloth/Qwen3.5-2B-GGUF Qwen3.5-2B-Q5_K_M.gguf"
  "unsloth/Qwen3.5-2B-GGUF Qwen3.5-2B-Q8_0.gguf"
  "unsloth/Qwen3.5-4B-GGUF Qwen3.5-4B-Q3_K_M.gguf"
  "unsloth/Qwen3.5-4B-GGUF Qwen3.5-4B-Q4_0.gguf"
  "unsloth/Qwen3.5-4B-GGUF Qwen3.5-4B-Q5_K_M.gguf"
  "unsloth/Qwen3.5-4B-GGUF Qwen3.5-4B-Q8_0.gguf"
  "unsloth/gemma-4-E2B-it-GGUF gemma-4-E2B-it-Q4_0.gguf"
  "unsloth/gemma-4-E2B-it-GGUF gemma-4-E2B-it-Q5_K_M.gguf"
  "unsloth/gemma-4-E2B-it-GGUF gemma-4-E2B-it-Q8_0.gguf"
  "unsloth/gemma-4-E4B-it-GGUF gemma-4-E4B-it-Q3_K_M.gguf"
  "unsloth/gemma-4-E4B-it-GGUF gemma-4-E4B-it-Q4_0.gguf"
  "unsloth/gemma-4-E4B-it-GGUF gemma-4-E4B-it-Q5_K_M.gguf"
  "unsloth/gemma-4-E4B-it-GGUF gemma-4-E4B-it-Q8_0.gguf"
)

case "$STAGE" in
  screen) entries=("${screen[@]}") ;;
  quants) entries=("${quants[@]}") ;;
  all)    entries=("${screen[@]}" "${quants[@]}") ;;
  *) echo "unknown stage: $STAGE" >&2; exit 2 ;;
esac

fetch() {
  local repo="$1" file="$2"
  local out="$MODELS/$file"
  if [[ -s "$out" ]]; then
    echo "cached  $file"
    return
  fi
  echo "fetch   $file"
  curl -L --fail --retry 3 --retry-delay 5 -C - \
    -o "$out.part" \
    "https://huggingface.co/$repo/resolve/main/$file"
  mv "$out.part" "$out"
}

export -f fetch
export MODELS
printf '%s\n' "${entries[@]}" | xargs -r -P 4 -n 2 bash -c 'fetch "$0" "$1"'

echo
echo "models in $MODELS"
du -sh "$MODELS"
