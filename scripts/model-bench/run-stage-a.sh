#!/usr/bin/env bash
# Run Stage A sequentially, waiting for each GGUF to finish downloading.
#
# This is intentionally a background orchestrator: the network on the
# development machine is much slower than the CPU, so the download and the
# benchmark have to overlap. Each model still runs alone, so the timings are
# not contaminated by a second llama-server.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SERVER="${SAM_BENCH_SERVER:-$ROOT/target/llama.cpp/build/bin/llama-server}"
if [[ ! -x "$SERVER" && -x "$HOME/src/llama.cpp/build/bin/llama-server" ]]; then
  SERVER="$HOME/src/llama.cpp/build/bin/llama-server"
fi
MODELS="${SAM_BENCH_MODELS:-$HOME/.cache/sam-bench/models}"
OUT="${SAM_BENCH_STAGE_A_OUT:-$ROOT/target/bench/stageA}"

# id:minimum bytes
entries=(
  "qwen3-0.6b-q4km:380000000"
  "qwen3-1.7b-q4km:1080000000"
  "qwen3-4b-q4km:2450000000"
  "qwen3.5-0.8b-q4km:520000000"
  "qwen3.5-2b-q4km:1250000000"
  "qwen3.5-4b-q4km:2700000000"
  "gemma4-e2b-q4km:3000000000"
  "gemma4-e4b-q4km:4900000000"
)

file_for() {
  case "$1" in
    qwen3-0.6b-q4km) echo "Qwen3-0.6B-Q4_K_M.gguf" ;;
    qwen3-1.7b-q4km) echo "Qwen3-1.7B-Q4_K_M.gguf" ;;
    qwen3-4b-q4km) echo "Qwen3-4B-Q4_K_M.gguf" ;;
    qwen3.5-0.8b-q4km) echo "Qwen3.5-0.8B-Q4_K_M.gguf" ;;
    qwen3.5-2b-q4km) echo "Qwen3.5-2B-Q4_K_M.gguf" ;;
    qwen3.5-4b-q4km) echo "Qwen3.5-4B-Q4_K_M.gguf" ;;
    gemma4-e2b-q4km) echo "gemma-4-E2B-it-Q4_K_M.gguf" ;;
    gemma4-e4b-q4km) echo "gemma-4-E4B-it-Q4_K_M.gguf" ;;
    *) return 1 ;;
  esac
}

wait_file() {
  local file="$1" min="$2"
  while true; do
    if [[ -f "$file" ]]; then
      local size
      size=$(stat -c%s "$file")
      if (( size >= min )); then
        return 0
      fi
    fi
    sleep 15
  done
}

wait_idle() {
  while pgrep -f "llama-server.*$MODELS" >/dev/null 2>&1; do
    sleep 10
  done
}

mkdir -p "$OUT"
for entry in "${entries[@]}"; do
  id="${entry%%:*}"
  min="${entry##*:}"
  file="$MODELS/$(file_for "$id")"
  run_dir="$OUT/$id"
  if [[ -f "$run_dir/summary.json" ]]; then
    echo "skip $id (already complete)"
    continue
  fi
  echo "waiting for $file"
  wait_file "$file" "$min"
  wait_idle
  echo "== running $id"
  if python3 "$ROOT/scripts/model-bench/run.py" \
    --models "$id" \
    --suite both \
    --server "$SERVER" \
    --threads 6 --ctx 2048 \
    --min-battery "${SAM_BENCH_MIN_BATTERY:-20}" \
    --out "$run_dir"; then
    :
  else
    code=$?
    if [[ "$code" -eq 42 ]]; then
      echo "battery threshold reached; stopping Stage A" | tee "$OUT/BATTERY_STOP"
      exit 42
    fi
    echo "run failed for $id (exit $code)" >&2
  fi
done
echo "stage A orchestrator done" > "$OUT/DONE"
