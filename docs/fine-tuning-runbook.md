# Fine-tuning runbook

**Status:** Runbook; the first local run is in progress · **Scope:** How to fine-tune Needle for Sam, step by step

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

```python
import json, cactus

rows = [json.loads(l) for l in open("heldout.jsonl") if l.strip()]

def run(weights=None):
    name_ok = args_ok = none_ok = 0
    wrong = []
    for r in rows:
        agent = cactus.Cactus(tools=r["tools"], weights=weights) if weights else cactus.Cactus(tools=r["tools"])
        calls = agent.complete(r["query"]).get("function_calls", [])
        exp = r["answers"]
        if not exp:
            if not calls:
                none_ok += 1
            else:
                wrong.append((r["query"], calls, exp))
            continue
        if calls and calls[0]["name"] == exp[0]["name"]:
            name_ok += 1
            if calls[0]["arguments"] == exp[0]["arguments"]:
                args_ok += 1
            else:
                wrong.append((r["query"], calls[0], exp[0]))
        else:
            wrong.append((r["query"], calls, exp))
    print(weights or "base", "name", name_ok, "args", args_ok, "none", none_ok, "/", len(rows))
    for w in wrong[:8]:
        print("  ", w)

run()
run("sam-needle.cact")
```

The exact-match result is the real metric. Loss is only a training signal.

## Hardware: CPU or a 3060

CPU is enough. This is a 121M model; a few hundred rows train in minutes to tens of minutes on a
modern laptop. The 3060 is an iteration-speed upgrade, not a capacity requirement.

### The 3060 path

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

Useful settings for a 6 GB laptop GPU:

```sh
export XLA_PYTHON_CLIENT_PREALLOCATE=false
export XLA_PYTHON_CLIENT_MEM_FRACTION=.5
```

Remote access: Tailscale inside WSL is the least fragile route.

```sh
curl -fsSL https://tailscale.com/install.sh | sh
sudo tailscale up --ssh
tailscale ip -4
```

Then `ssh <user>@<tailscale-ip>`. Run training inside `tmux` so an SSH drop does not kill it.

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
  then merges and quantises to **4 bits** at export.
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
| Out of memory | Batch or sequence too large | `--batch-size 8`, `--max-len 768` |

## First run record (2026-11-06)

A first local run on the development laptop, for continuity:

| Field | Value |
|---|---|
| Data | 360 rows, 40 tools, 4-tool subsets |
| Holdout | 36 rows for validation |
| Sequence | padded to 256, cap 1024 |
| LoRA | rank 16, alpha 32, 5 weight groups |
| Schedule | 420 steps, warmup 21, cosine decay |
| Epoch 1 | train 1.2229, val 1.1683 |
| Epoch 2 | train 0.7611, val 1.1268 |

The run is healthy so far: both curves are falling, and the train/validation gap is normal early.
The next step is to let it finish, build the `.cact`, and compare base versus tuned on the
held-out set.
