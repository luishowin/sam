# Speech Input Guide (for the coding agent)

Add local, offline, English-only speech-to-text to the launcher's search bar. The user presses the launcher hotkey, the bar appears, and a mic control inside it lets them speak a query that appears as text in the input field. It must feel instant.

## Ground rules for the agent

1. **Do not invent APIs.** This guide names crates and models, but exact function signatures change between versions. Before writing code against a crate, read its `Cargo.toml` version on crates.io / docs.rs and the `examples/` directory in its repo. Copy the usage patterns from those examples.
2. **Make small, compilable steps.** Get one stage working and tested (capture, then resample, then transcribe, then UI) before adding the next.
3. **Measure, don't guess.** Log timings at each stage (see "Instrumentation") and report real numbers.
4. **Everything runs locally.** No network calls at runtime. Model files are downloaded once at setup.

The transcript this pipeline produces enters the same tiered router as typed text: Tier 0
first, then an optional local router on a miss. This guide covers capture, endpointing and
transcription only; the tier contracts are in [`../model-tiers.md`](../model-tiers.md).

## Target environment

- OS: Fedora Linux, PipeWire audio.
- CPU: AMD Ryzen 5 5600U (6 cores / 12 threads), 16 GB RAM. No dedicated GPU, so CPU inference only.
- Language: Rust.
- Language of speech: English only.

## Models

Use two models in a hybrid pipeline.

| Role | Model | Why |
|---|---|---|
| Live partial text while speaking | **Moonshine Small Streaming (English)** | True streaming, low latency, small. |
| Final, accurate text after speaking stops | **NVIDIA Parakeet TDT 0.6B v3 (int8 ONNX)** | Higher accuracy, better punctuation and names, fast on CPU. Not a streaming model, so it runs on the finished clip. |

Both are available through one crate (below). Make the Parakeet second pass **optional via a config flag**, because it needs to be benchmarked on this CPU before it is enabled by default. If it adds noticeable delay, ship Moonshine alone.

### Getting the model files

- The `transcribe-rs` README / docs.rs page lists download sources for each model (a hosted blob store and Hugging Face). Use those links rather than guessing URLs.
- Store models in `~/.local/share/<app-name>/models/`.
- Expected directory layouts (verify against the crate docs for the version you pin):

```
models/parakeet-tdt-0.6b-v3-int8/
├── encoder-model.int8.onnx
├── decoder_joint-model.int8.onnx
├── nemo128.onnx
└── vocab.txt

models/moonshine-streaming/moonshine-small-streaming-en/
├── encoder.onnx
├── decoder.onnx
├── streaming_config.json
└── tokenizer.json
```

The Moonshine layout above is the documented layout for the tiny streaming variant. Confirm the small variant uses the same file names.

- Write a small `setup` subcommand or script that downloads and verifies the files, and a clear error message if the models are missing at runtime.

## Crate

Use **`transcribe-rs`** with the `onnx` feature. It wraps both Parakeet and Moonshine (including the streaming variant) behind one interface.

```toml
transcribe-rs = { version = "0.3", features = ["onnx"] }
```

Pin the exact version after checking the latest release. Read `examples/parakeet.rs` and `examples/moonshine_streaming.rs` in its repo and base the integration on those.

Fallback if `transcribe-rs` is a problem: `parakeet-rs` for Parakeet only. Moonshine would then need another route, so prefer `transcribe-rs`.

Notes:
- The ONNX Runtime binary is fetched or linked at build time by the `ort` dependency. If the build fails on this, read the `ort` docs for how to provide the runtime on Fedora.
- Models expect **16 kHz, mono** audio. Check whether the crate wants `f32` samples or 16-bit PCM for each engine and convert accordingly.

## Architecture

```
mic (PipeWire via cpal)
   -> ring buffer (always-on while bar is open)
   -> downmix to mono, resample to 16 kHz
   -> Moonshine Streaming  -> partial text -> UI input field (live)
   -> (also append samples to full-utterance buffer)

on stop (mic toggled off, key released, or silence endpoint):
   -> flush Moonshine, take its final text
   -> if parakeet_second_pass enabled: run full-utterance buffer through Parakeet
        -> replace input field text with Parakeet result
   -> otherwise keep Moonshine's final text
```

### Preloading is the most important requirement

"Instant" depends on load time far more than inference time.

- Load **both models once at launcher startup** in a background thread. Never load a model on hotkey press.
- If the launcher is a persistent process, hold the loaded models in that process (for example behind `Arc<Mutex<_>>` or owned by a dedicated worker thread that receives audio over a channel).
- If the launcher process is spawned fresh on each hotkey press, you need a separate long-lived daemon that owns the models and a local Unix socket for the launcher to talk to. Ask the user which case applies before building the daemon.
- Warm the models by running a short silent dummy clip through each after loading, so the first real use is not slower than later ones.
- Expose a "models ready" state to the UI. If the user presses the mic before loading finishes, show a loading indicator and start buffering audio, then process it once ready.

### Dedicated inference thread

Run inference on its own thread. Never block the UI thread. Communicate with channels (`std::sync::mpsc` or `crossbeam-channel`). Suggested messages:

- `StartListening`
- `AudioChunk(Vec<f32>)`
- `StopListening`
- Events back to UI: `Partial(String)`, `Final(String)`, `Error(String)`, `ModelsReady`

### Threads and CPU

- Moonshine streaming: start with 2 to 4 inference threads.
- Parakeet runs after streaming stops, so it can use more threads (try 6). Make thread counts configurable and benchmark.

## Audio capture

- Use `cpal`. On Fedora it reaches PipeWire through the ALSA compatibility layer. You will likely need the `alsa-lib-devel` package to build.
- Open the default input device. Handle devices that only offer stereo or 44.1 / 48 kHz by downmixing and resampling.
- Resample to 16 kHz with the `rubato` crate (or another well-tested resampler). Do not write a naive resampler.
- **Start capturing the instant the bar opens**, and keep a rolling pre-buffer of roughly 0.5 to 1 second. When the user toggles the mic, include that pre-buffer so the first syllable is not clipped.
- Keep the audio stream alive while the bar is open rather than opening and closing the device on every press, which adds latency. Close it when the bar hides.
- Convert samples to the format the crate requires (`f32` normalized to [-1, 1], or `i16`), per the crate examples.

## Endpointing (when to stop)

Start simple, then improve:

1. **v1:** manual stop. Mic button toggles listening on and off. Optional push-to-talk key.
2. **v2:** silence detection. Stop automatically after about 0.8 to 1.2 seconds of silence once speech has been detected. A simple energy threshold is enough to start. Make the timeout configurable. Do not add a heavy VAD dependency unless the energy approach proves unreliable.

## UI integration

- The mic control lives inside the search bar. Show clear states: idle, loading, listening, processing.
- Live partial text replaces the input field contents as it arrives. Keep the cursor at the end.
- When the Parakeet pass finishes, swap the text in place. Avoid flicker: only update if the text changed.
- If the user starts typing while listening, stop listening and keep their typed text.
- Pressing Escape cancels listening and discards audio.
- After a final result, normal search should trigger as it does for typed text. Do not auto-launch a result from speech alone.

## Instrumentation

Log, at debug level, with timestamps:

- Model load time (each model).
- Time from mic press to first captured sample.
- Time from first speech to first Moonshine partial.
- Moonshine finalization time after stop.
- Parakeet inference time for the clip, plus clip duration.
- Peak memory of the process after loading both models.

Report these numbers when the feature works, so the user can decide whether to keep the Parakeet second pass.

## Error handling

- Missing models: clear message plus how to run setup.
- No input device or device busy: show a non-blocking error in the UI and let the user keep typing.
- Inference errors: log, surface `Error`, return the UI to idle. Never crash the launcher over a speech failure.
- Empty transcription: do nothing, return to idle.

## Build order

1. Spike: standalone binary that loads Parakeet and transcribes a WAV file. Confirm it works and time it.
2. Spike: standalone binary that streams a WAV file through Moonshine Streaming in chunks and prints partials.
3. Add `cpal` capture, resample to 16 kHz, and feed live mic audio to Moonshine. Print partials to the terminal.
4. Add the full-utterance buffer and the Parakeet second pass. Compare accuracy and timing against Moonshine alone.
5. Move inference onto its own thread with channels, with startup preloading and warm-up.
6. Integrate into the launcher UI (mic button, states, live text).
7. Add silence endpointing.
8. Add config options and the setup/download command.

## Acceptance criteria

- Pressing the hotkey then the mic starts capture with no perceptible delay once models are loaded.
- Live text appears while speaking.
- Short queries (3 to 8 words) finish with the final text within a fraction of a second of stopping. Report the measured number.
- The launcher UI never freezes during inference.
- The launcher starts without models and degrades gracefully (mic disabled, typing still works).
- No network access is needed at runtime.

## Questions to ask the user before starting

1. Is the launcher a persistent process, or spawned on every hotkey press?
2. Which UI toolkit is it built on (GTK4, iced, egui, other)? This affects how to deliver events from the inference thread to the UI.
3. Should the mic be toggle-to-talk, push-to-talk, or both?
