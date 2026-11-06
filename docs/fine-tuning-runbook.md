# Fine-tuning runbook

**Status:** Runbook; first tune complete and measured (2026-11-07) · **Scope:** How to fine-tune Needle for Sam, step by step

This is the practical companion to [`fine-tuning.md`](fine-tuning.md). That document says what
data is allowed and why; this one says which commands to run, in what order, and what breaks.

---

## What Needle is, and is not

Needle 3 is a 29–121M parameter tool-calling model from Cactus. It is not an LLM in the chat
sense: it reads a query and a list of tool schemas and returns a JSON call, or `[]` when no
declared tool applies.

It runs on the **Cactus engine**, a native C/C++ runtime under 1 MB, shipped through the
`cactus-compute` Python package. The weights are a single `.cact` file.

- **No Ollama.**
- **No llama.cpp.**
- **No GGUF.**
- Inference does not need JAX; only fine-tuning and export do.
- The engine and the base `needle3.cact` are downloaded once and cached under `~/.cache/cactus/`.

The engine is weights-agnostic. A tuned archive runs on the same engine, with the same API.

## Prerequisites

| Piece | Requirement |
|---|---|
| Python | 3.11 or 3.12; 3.13 may work but JAX wheels lag |
| Runtime | `pip install cactus-compute` |
| CPU training | `pip install "cactus-compute[train]"` |
| NVIDIA training | `pip install "cactus-compute[train,gpu]"` |
| Apple training | `pip install "cactus-compute[train,metal]"` |
| Disk | A few GB for the engine, base checkpoint and checkpoints |
| Package | Pin the same `cactus-compute` version wherever you train and build |

Set `CACTUS_TELEMETRY=0` or `DO_NOT_TRACK=1` if you want no anonymous usage counts.

## The three files

| File | What it is |
|---|---|
| `tools.json` | The catalogue of tool definitions: `name`, `description`, `parameters` |
| `data.jsonl` | Training rows, one JSON object per line |
| `heldout.jsonl` | Rows never used for training, for the final base-versus-tuned comparison |

Do not call `data.jsonl` a JSON array. It is JSON Lines.

A row:

```json
{"query":"set a timer for 10 minutes","tools":[{"name":"timer.set","description":"Set a timer","parameters":{"type":"object","properties":{"duration":{"type":"string"}},"required":["duration"]}}],"answers":[{"name":"timer.set","arguments":{"duration":"10m"}}],"reasoning":"'10 minutes' -> duration 10m"}
```

Fields:

- `query` — the utterance, as typed or transcribed;
- `tools` — the candidate subset for this row, not the full catalogue;
- `answers` — the exact calls, or `[]` for a refusal;
- `reasoning` — one short line deriving each argument from the query;
- `system` — optional environment facts, such as `date: 2026-11-06 Fri 09:00; locale: en-GB`.

## The candidate-subset rule

Needle 3 reads the whole declared toolset. It has **no retrieval head**. A 40-tool catalogue in
one prompt is too long for the 1024-token context and degrades selection.

So:

- `tools.json` may hold the full catalogue;
- every training row carries **4–8 tools**: the correct one, 2–4 plausible confusions, 1–2
  irrelevant;
- at inference, Sam's candidate retrieval builds the same kind of subset before calling Needle.

Train on the full catalogue and deploy on a subset, and the distributions will not match.

## Data rules

1. Arguments contain only values present in the query. Never invent a file, app, number or URL.
2. Optional fields with no evidence are omitted, not filled with placeholders.
3. Include `[]` negatives; about one row in six.
4. Include missing-argument rows that call the `clarify` pseudo-tool, not a guessed call.
5. Include STT noise: dropped words, homophones, digits versus words.
6. Include ambiguous near-misses and the correct choice.
7. Keep each row short enough to fit `--max-len 1024`.
8. Use both British and American spellings: minimise/minimize, centre/center.

Sizing, from the Cactus guide:

- a few hundred clean rows measurably improve **tool selection**;
- argument **grounding** needs more, on the order of a thousand or more, with varied values and
  reasoning lines;
- 200 rows at batch 16 is about 13 steps per epoch; three epochs barely moves a rank-16 adapter.
  Use 10–30 epochs on a small set.

## Local run

```sh
cd "/path/to/needle resources"
uv venv --python 3.12 .venv
source .venv/bin/activate
uv pip install -U pip
uv pip install "cactus-compute[train]"
export CACTUS_TELEMETRY=0
```

Smoke-test the base model with a 4-tool subset, not the full catalogue:

```sh
python - <<'PY'
import json, cactus
tools = json.load(open("tools.json"))
subset = [t for t in tools if t["name"] in {"timer.set", "wifi.set", "window.minimize", "probe.uptime"}]
agent = cactus.Cactus(tools=subset)
for q in ["set a timer for 10 minutes", "how long has my laptop been on"]:
    print(q, "->", agent.complete(q)["function_calls"])
PY
```

Fine-tune:

```sh
cactus finetune data.jsonl --epochs 20 --val-split 0.1 --seed 0 --out adapter.safetensors
```

Build:

```sh
cactus build --lora adapter.safetensors --layers 20 --out sam-needle.cact
cactus build --lora adapter.safetensors --layers 8  --out sam-needle-8l.cact
```

Run:

```python
import cactus
agent = cactus.Cactus(tools=subset, weights="sam-needle.cact")
print(agent.complete("set a timer for 10 minutes"))
```

`--layers N` slices a smaller subnetwork from the same adapter. Test accuracy against size; the
shipped 20-layer base is 29 MB, and an 8-layer build is smaller and faster.

## Reading the loss

The loss covers only the reasoning line and the call, so it starts near 1.0: most of the call is
boilerplate the base already predicts.

- Judge the **epoch trend**, not individual steps.
- Stop when held-out loss rises while training loss keeps falling.
- A flat curve after a few hundred steps means undertrained, not broken: raise epochs first,
  then the learning rate.
- A widening gap with rising validation loss means overfit: add data or stop earlier.

The held-out split used by `--val-split` is not the frozen `heldout.jsonl`. Keep those separate.

## Evaluating base versus tuned

Run every held-out row through both models and compare the first call on name and arguments.
The repository harness does this and reports both raw and normalised argument matches:

```sh
cd "/path/to/needle resources"
.venv/bin/python /path/to/sam/scripts/evaluate-needle.py \
  --data heldout.jsonl --tools tools.json \
  --weights base --weights sam-needle.cact
```

It pins the system date fact (`date: 2026-11-06 Fri 09:00; locale: en-GB`) so repeated runs are
identical; pass `--system auto` to use the current clock. Normalisation handles case, whitespace,
durations, clock times and numbers. It never rewrites the tool name, and it never adds or removes
a path extension: `presentation` and `presentation.pdf` remain different values.

Raw exact match is the real metric. The normalised match is a secondary reading that separates
formatting drift from wrong grounding. Loss is only a training signal.

## Hardware: CPU or a small NVIDIA GPU

CPU is enough. This is a 121M model; a few hundred rows train in minutes to tens of minutes on a
modern laptop. A 4 GB laptop GPU (RTX 3050) trained the first run in about 17 minutes at batch 8.
A 3060 is an iteration-speed upgrade, not a capacity requirement.

### The small-GPU path

On the Windows host:

1. Install the **NVIDIA Windows driver** only. Do not install a Linux NVIDIA driver inside WSL.
2. Install WSL2 and Ubuntu 24.04:
   ```powershell
   wsl --install -d Ubuntu-24.04
   wsl --update
   ```
3. Inside WSL, `nvidia-smi` must show the GPU.

Inside WSL:

```sh
sudo apt update && sudo apt install -y python3.12 python3.12-venv python3-pip tmux rsync
python3.12 -m venv ~/needle/.venv
source ~/needle/.venv/bin/activate
pip install -U pip
pip install "cactus-compute[train,gpu]"
python -c "import jax; print(jax.devices())"    # must show CudaDevice
```

Useful settings for a 4–6 GB laptop GPU:

```sh
export XLA_PYTHON_CLIENT_PREALLOCATE=false
export XLA_PYTHON_CLIENT_MEM_FRACTION=.5
export XLA_FLAGS=--xla_disable_hlo_passes=constant_folding
```

The last line avoids an XLA compile-time constant-folding spike on the engram tensor that can push
host RSS past 10 GB. Use `--batch-size 8` on 4 GB.

Set WSL memory before training. The default 8 GB VM is not enough for this model's compile peak:

```ini
# C:\Users\<name>\.wslconfig
[wsl2]
memory=11GB
swap=4GB
```

Apply it with `wsl --shutdown` from PowerShell, then reopen Ubuntu.

If `cactus build` fails with `RESOURCE_EXHAUSTED: Failed to allocate ... pinned host staging
buffer`, build on CPU instead. The export does not need the GPU:

```sh
JAX_PLATFORMS=cpu cactus build --lora adapter.safetensors --layers 20 --out sam-needle.cact
```

Remote access: OpenSSH over Tailscale inside WSL is the least fragile route.

```sh
sudo apt install -y openssh-server
sudo systemctl enable --now ssh
curl -fsSL https://tailscale.com/install.sh | sh
sudo tailscale up
sudo systemctl enable --now tailscaled
tailscale ip -4
```

Use plain OpenSSH over the tailnet address rather than Tailscale SSH: the default tailnet ACL
only grants a member access to their own devices, so `--ssh` needs an ACL change before a second
person can log in. Add the client's public key to `~/.ssh/authorized_keys` and connect with
`ssh <user>@<tailscale-ip>`. Run training inside `tmux` so an SSH drop does not kill it.

Copy the data over:

```sh
rsync -av --exclude '.venv' --exclude 'checkpoints' --exclude '*.safetensors' --exclude '*.cact' \
  "resources/" <user>@<tailscale-ip>:~/needle/
```

Copy the adapter back, then build locally or remotely. Keep the `cactus-compute` version
identical on both machines.

Laptop gotchas: sleep kills training, Windows Update can reboot, and WSL can take half the host
memory. Keep it plugged in, pause updates, and set `.wslconfig` if needed.

## Artifacts and versioning

- Local fine-tuning trains LoRA rank 16 on the attention projections at the full 20 layers,
  then merges and quantises to **4 bits** at export. The first run produced a 7.9 MB adapter, a
  63.4 MB 20-layer archive and a 27.4 MB 8-layer archive; the base is 35.3 MB at 2 bits.
- The stock trainer writes the adapter only after the last epoch. Add an epoch-level
  `write_adapter` call, or run inside tmux, when an interruption would be expensive; a crashed
  run otherwise loses all progress.
- The **confidence head is not trained** locally. `Cactus(weights=...)` reports
  `confidence = None`. Either calibrate a threshold from your own validation, keep the base
  model for gating, or use the hosted platform path, which trains the head at 2 bits.
- The tokenizer is untouched.
- The `.cact` format is tied to the engine version. An archive from a newer package may not load
  in an older one. Pin `cactus-compute` and rebuild when the engine changes.
- Record the dataset hash, seed, package version and base checkpoint with the artifact.
- Never replace a shipped artifact silently.

## Troubleshooting

| Symptom | Cause | Fix |
|---|---|---|
| `failed to load weights` | `.cact` built by a different engine version | Rebuild with the current package |
| Loss flat at its starting value | Undertrained | More epochs, then a higher learning rate |
| Tuned model calls a tool on everything | No `[]` rows | Add negatives |
| Correct tool, wrong values | Too little grounding data | More varied values and reasoning lines; try rank 32 |
| `jax.devices()` shows CPU | CUDA extra missing or wrong Python | Reinstall `[train,gpu]`, check `nvidia-smi` inside WSL |
| Out of memory (GPU) | Batch or sequence too large | `--batch-size 8`, `--max-len 768` |
| Host OOM killer kills `cactus` | WSL VM too small, or XLA constant folding spikes host RSS | Raise `.wslconfig` memory; set `XLA_FLAGS=--xla_disable_hlo_passes=constant_folding`; use `--batch-size 8` |
| `RESOURCE_EXHAUSTED ... pinned host staging buffer` during build | WSL/JAX pinned-memory pool exhausted | `JAX_PLATFORMS=cpu cactus build ...` |
| Tailscale returns `node nodekey ... already exists` | The node key is registered to another tailnet | Remove `/var/lib/tailscale/tailscaled.state`, restart `tailscaled`, join with an auth key from the target tailnet |

## First run record (2026-11-07)

The first completed tune ran on a borrowed Windows laptop over SSH. The earlier CPU attempt was
interrupted by a power loss; this is the first artifact.

### Environment

| Field | Value |
|---|---|
| Host | Windows + WSL2 Ubuntu 24.04 |
| GPU | NVIDIA RTX 3050 Laptop, 4 GB |
| WSL resources | 8 cores, 11 GB RAM, 4 GB swap (raised from the 8 GB default) |
| Runtime | `cactus-compute` 4.0.1, JAX 0.12.0.1, Python 3.12.15 |

Training used `XLA_FLAGS=--xla_disable_hlo_passes=constant_folding`; without it the compiler's
constant-folding pass grew host RSS past 10 GB and the OOM killer stopped the run.

### Data and schedule

| Field | Value |
|---|---|
| Data | 360 rows, 40 tools, 4-tool subsets |
| Validation split | 36 rows (`--val-split 0.1`, seed 0) |
| Frozen held-out | 40 rows (32 positive, 8 negative) |
| Sequence | padded to 256, cap 1024 |
| LoRA | rank 16, alpha 32, 5 weight groups |
| Schedule | 20 epochs, batch 8, 820 steps, warmup 41, cosine decay, clip 1.0 |

### Validation

| Epoch | Train | Val | Epoch | Train | Val |
|---:|---:|---:|---:|---:|---:|
| 1 | 1.2159 | 1.1988 | 11 | 0.2330 | 0.2982 |
| 2 | 0.7126 | 1.0819 | 12 | 0.2703 | 0.2802 |
| 3 | 0.4414 | 0.9039 | 13 | 0.3574 | 0.2679 |
| 4 | 0.9632 | 0.7384 | 14 | 0.2164 | 0.2593 |
| 5 | 0.5182 | 0.6175 | 15 | 0.1355 | 0.2545 |
| 6 | 0.3646 | 0.5332 | 16 | 0.2442 | 0.2498 |
| 7 | 0.2855 | 0.4651 | 17 | 0.2911 | 0.2464 |
| 8 | 0.5867 | 0.4031 | 18 | 0.1744 | 0.2452 |
| 9 | 0.3914 | 0.3593 | 19 | 0.1344 | **0.2447** |
| 10 | 0.3618 | 0.3248 | 20 | 0.1992 | 0.2457 |

Validation bottomed out at epoch 19; the last two epochs are within noise. The exported artifact is
the final epoch (20). The trainer also scored **23/36** of the validation split exact on the W4
weights the archive ships.

### Frozen held-out comparison

Fixed system fact `date: 2026-11-06 Fri 09:00; locale: en-GB`, so the numbers reproduce. The
argument comparison is reported raw and after normalisation.

| Model | Name | Args raw | Args normalised | `[]` | False pos | False neg | Median latency |
|---|---:|---:|---:|---:|---:|---:|---:|
| base Needle 3 | 23/32 | 20/32 | 22/32 | 7/8 | 1 | 2 | 227 ms |
| tuned, 20 layers | **26/32** | **24/32** | **26/32** | **8/8** | **0** | 2 | 267 ms |
| tuned, 8 layers | 14/32 | 10/32 | 11/32 | 4/8 | 4 | 1 | — |

The 20-layer tune is the candidate: +3 tool names, +4 grounded arguments, +1 negative, and no
false positives. The 8-layer slice collapses and is not viable. Latency is a warm 40-call median
on the development laptop with a four-tool subset, not a p95 in the sidecar; the 250 ms p95 target
is still unverified.

### Artifacts

| File | Size | SHA-256 |
|---|---:|---|
| `adapter.safetensors` | 7.9 MB | `e63781cbd1993556be3d3e96f41bb3a88e0646bbd0419590133193b12225207c` |
| `sam-needle.cact` (20L, W4A8) | 63.4 MB | `1af7d2b01ca9d318f6db208a7a3af0856d060a0bab5c774a9ef08a5d6e27a0d8` |
| `sam-needle-8l.cact` (8L, W4A8) | 27.4 MB | `5620b7175a9f63176b5fcfa7394f6d91a6482bf02dc8af246718a97d76f6c4c1` |
| `needle3.cact` (base, 2-bit) | 35.3 MB | `c9d915eca282ed42d1a09b143b592adb4cc6744ffe2d294adf5cfc5548170c38` |

```text
data.jsonl    812b4c9317d6540ded7da9275548f5c8c7c730bc310068b4353684dc79b46704
heldout.jsonl c34f1b5fdfedf5e015d6376885b82c4601cdfcc5723a3fb769ecb1e29624ea7e
tools.json    ea647da05783bd4a9151dbefbf1bab2bde841e8b017671a6d71a1f45174a68f4
```

Build (CPU, because the GPU build hit a WSL pinned-host-memory error):

```sh
JAX_PLATFORMS=cpu cactus build --lora adapter.safetensors --layers 20 --out sam-needle.cact
```

### What went wrong, and the fixes

- The first attempt ran on the development laptop and was interrupted by a power loss.
  `cactus finetune` writes the adapter only after the last epoch, so nothing survived. The second
  run added an epoch-level `write_adapter` call as insurance; the stock trainer still has no
  resume support.
- WSL had 8 GB. The trainer was killed at 6.7 GB RSS. Raising `.wslconfig` to 11 GB fixed the
  ceiling.
- The next attempt reached 10.6 GB during XLA constant folding on the engram tensor and was
  killed. `XLA_FLAGS=--xla_disable_hlo_passes=constant_folding` kept the run near 7 GB.
- `cactus build` on the GPU failed with `RESOURCE_EXHAUSTED: Failed to allocate ... pinned host
  staging buffer`; building on CPU is the reliable path.
- The same rows produced ±1 different calls between runs because the default system prompt
  includes the current date and time. The evaluator now pins the date fact.

### Decision

Keep the 20-layer candidate for the real-use comparison. It is **not shipped**: the gate in
[`evaluation.md`](evaluation.md) still needs the real-use set and the safety suite, and the
latency target needs a sidecar p95 measurement.
