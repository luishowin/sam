#!/usr/bin/env bash
# Score every completed Stage A model and write the combined table.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${SAM_BENCH_STAGE_A_OUT:-$ROOT/target/bench/stageA}"
for dir in "$OUT"/*/; do
  [[ -f "$dir/summary.json" ]] || continue
  echo "scoring $(basename "$dir")"
  python3 "$ROOT/scripts/model-bench/score.py" "$dir"
done
python3 "$ROOT/scripts/model-bench/compare.py" "$OUT" --out "$OUT/summary.md"
echo "wrote $OUT/summary.md"
