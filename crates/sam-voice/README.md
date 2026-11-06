# sam-voice

Local, offline, English-only push-to-talk speech-to-text for Sam.

The crate uses **transcribe-rs 0.3.12** with the `onnx` feature and
**Moonshine Small Streaming English**. Parakeet and whisper.cpp are not part of
this build. Runtime recognition never uses the network.

## Build prerequisites (Fedora)

```sh
sudo dnf install alsa-lib-devel openssl-devel
```

`alsa-lib-devel` is needed by cpal. `openssl-devel` is needed at build time by
`ort`'s binary downloader; the resulting binary does not link a system OpenSSL
for inference.

## Model setup

```sh
cargo run -p sam-voice -- setup
```

This downloads the verified archive from
`https://blob.handy.computer/moonshine-small-streaming-en.tar.gz`, checks its
SHA-256, and installs it to:

```text
~/.local/share/sam/models/moonshine-streaming/moonshine-small-streaming-en/
├── frontend.ort
├── encoder.ort
├── adapter.ort
├── cross_kv.ort
├── decoder_kv.ort
├── streaming_config.json
└── tokenizer.bin
```

For an offline machine, copy the archive over and run
`sam-voice setup --archive moonshine-small-streaming-en.tar.gz`.

Check the installation with:

```sh
cargo run -p sam-voice -- status
```

## WAV spike

```sh
cargo run -p sam-voice --release -- transcribe speech.wav
```

The reader accepts any WAV hound can decode, downmixes it to mono, and
resamples it to 16 kHz. Debug logging (`RUST_LOG=sam_voice=debug`) prints stage
timings.

## Panel API

```rust
let engine = Rc::new(VoiceEngine::spawn_default()?);

// In a glib::timeout_add_local callback (every ~50 ms):
for event in engine.events() {
    match event {
        Event::ModelsLoading => show_loading(),
        Event::ModelsReady => enable_mic(),
        Event::Listening => show_listening(),
        Event::Partial(text) => set_entry_text(&text),
        Event::Final(text) => { set_entry_text(&text); focus_entry(); }
        Event::Failed(text) => { show_error(&text); go_idle(); }
    }
}

// Push-to-talk:
engine.start()?;   // enqueue only
engine.stop()?;    // enqueue only
engine.cancel()?;  // discard, no Final
```

In the panel, both the microphone button and **hold-Tab** call `start()` on
press and `stop()` on release. If the model is still warming up, the panel
remembers the held key and starts listening once `ModelsReady` arrives.

`VoiceEngine` is `Send` but not `Sync` (it owns the event receiver), so keep it
in an `Rc` on the GTK main thread. Model loading happens on the worker at
startup and is reported through `ModelsLoading`/`ModelsReady`.

## Design note

transcribe-rs 0.3.12 exposes whole-clip `transcribe_raw` for the Moonshine
streaming model, not an incremental decoder API. Partials are produced by
re-running the model over the accumulated 16 kHz utterance every
`VoiceConfig::partial_interval` (default 800 ms). The final pass runs over the
complete clip, or reuses the last partial when no new audio arrived.

## CLI

```text
sam-voice status
sam-voice setup [--archive PATH]
sam-voice transcribe <wav> [--threads N]
sam-voice mic
```

`sam-voice mic` is a manual smoke test: press Enter to start and Enter to stop.
