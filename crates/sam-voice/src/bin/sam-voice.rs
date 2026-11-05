//! Small command-line spike for the speech pipeline.
//!
//! ```text
//! sam-voice status
//! sam-voice setup
//! sam-voice setup --archive moonshine-small-streaming-en.tar.gz
//! sam-voice transcribe samples/jfk.wav
//! sam-voice mic
//! ```

use std::env;
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use sam_voice::{
    default_models_root, discover_model_dir, download_and_install, install_archive,
    layout_description, model_status, model_status_for_root, read_wav_16k_mono, Event,
    SetupOutcome, VoiceConfig, VoiceEngine, SAMPLE_RATE,
};
use transcribe_rs::onnx::moonshine::StreamingModel;
use transcribe_rs::onnx::Quantization;
use transcribe_rs::{SpeechModel, TranscribeOptions};

fn main() -> ExitCode {
    init_logging();
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let options = Options::parse()?;
    match options.command.as_str() {
        "status" => status(&options),
        "setup" => setup(&options),
        "transcribe" => transcribe(&options),
        "mic" => mic(&options),
        "help" => {
            print_help();
            Ok(())
        }
        other => Err(format!("unknown command `{other}`; try `sam-voice help`")),
    }
}

struct Options {
    command: String,
    positional: Vec<String>,
    models_root: PathBuf,
    threads: usize,
    archive: Option<PathBuf>,
}

impl Options {
    fn parse() -> Result<Self, String> {
        let mut positional = Vec::new();
        let mut models_root = None;
        let mut threads = None;
        let mut archive = None;

        let mut arguments = env::args().skip(1);
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "-h" | "--help" => positional.insert(0, "help".to_string()),
                "--models-root" => {
                    models_root = Some(PathBuf::from(
                        arguments
                            .next()
                            .ok_or_else(|| "--models-root needs a path".to_string())?,
                    ));
                }
                "--threads" => {
                    let value = arguments
                        .next()
                        .ok_or_else(|| "--threads needs a number".to_string())?;
                    threads = Some(
                        value
                            .parse::<usize>()
                            .map_err(|_| format!("invalid --threads value `{value}`"))?,
                    );
                }
                "--archive" => {
                    archive = Some(PathBuf::from(
                        arguments
                            .next()
                            .ok_or_else(|| "--archive needs a path".to_string())?,
                    ));
                }
                other if other.starts_with('-') => {
                    return Err(format!("unknown option `{other}`"));
                }
                other => positional.push(other.to_string()),
            }
        }

        let command = positional.first().cloned().unwrap_or_else(|| "help".into());
        Ok(Self {
            command,
            positional,
            models_root: models_root.unwrap_or_else(default_models_root),
            threads: threads.unwrap_or(4),
            archive,
        })
    }
}

fn status(options: &Options) -> Result<(), String> {
    let directory = discover_model_dir(&options.models_root);
    let status = model_status(&directory);

    println!("models root: {}", options.models_root.display());
    println!("model dir:   {}", directory.display());
    if status.is_present() {
        println!("status:      ready (Moonshine Small Streaming English)");
    } else {
        println!("status:      missing");
        println!("missing:     {}", status.missing().join(", "));
        println!();
        println!("{}", layout_description(&options.models_root));
    }
    Ok(())
}

fn setup(options: &Options) -> Result<(), String> {
    let outcome = match &options.archive {
        Some(archive) => install_archive(archive, &options.models_root),
        None => download_and_install(&options.models_root),
    }
    .map_err(|error| error.to_string())?;

    match outcome {
        SetupOutcome::AlreadyPresent { dir } => {
            println!("model already installed: {}", dir.display());
        }
        SetupOutcome::Installed { dir } => {
            println!("model installed: {}", dir.display());
        }
    }
    println!("status: ready (Moonshine Small Streaming English)");
    Ok(())
}

fn transcribe(options: &Options) -> Result<(), String> {
    let path = options
        .positional
        .get(1)
        .ok_or_else(|| "usage: sam-voice transcribe <16k-mono.wav>".to_string())?;
    let path = PathBuf::from(path);

    let status = model_status_for_root(&options.models_root);
    if !status.is_present() {
        return Err(format!(
            "{}\n\n{}",
            sam_voice::VoiceError::ModelsMissing {
                dir: status.dir().to_path_buf(),
                missing: status.missing().to_vec(),
            },
            layout_description(&options.models_root)
        ));
    }

    let load_wav = Instant::now();
    let audio = read_wav_16k_mono(&path).map_err(|error| error.to_string())?;
    println!(
        "wav:        {} ({:.2}s, {} Hz, {} ch -> {} samples at {} Hz) in {:?}",
        path.display(),
        audio.source_duration.as_secs_f64(),
        audio.source_sample_rate,
        audio.source_channels,
        audio.samples.len(),
        SAMPLE_RATE,
        load_wav.elapsed()
    );

    let load_start = Instant::now();
    let mut model = StreamingModel::load(
        status.dir(),
        options.threads.max(1),
        &Quantization::default(),
    )
    .map_err(|error| format!("could not load model: {error}"))?;
    println!("load:       {:?} ({} threads)", load_start.elapsed(), options.threads);

    let inference_start = Instant::now();
    let result = model
        .transcribe_raw(&audio.samples, &TranscribeOptions::default())
        .map_err(|error| format!("inference failed: {error}"))?;
    let inference = inference_start.elapsed();

    let audio_seconds = audio.samples.len() as f64 / SAMPLE_RATE as f64;
    let speedup = if inference.as_secs_f64() > 0.0 {
        audio_seconds / inference.as_secs_f64()
    } else {
        f64::INFINITY
    };

    println!(
        "inference:  {:?} for {:.2}s audio ({:.2}x realtime)",
        inference, audio_seconds, speedup
    );
    println!();
    println!("{}", result.text.trim());
    Ok(())
}

fn mic(options: &Options) -> Result<(), String> {
    let config = VoiceConfig {
        models_root: options.models_root.clone(),
        threads: options.threads.max(1),
        ..VoiceConfig::default()
    };
    let engine = VoiceEngine::spawn(config).map_err(|error| error.to_string())?;

    // A dedicated stdin reader means partial text keeps printing while we wait
    // for the user to press Enter.
    let (line_tx, line_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let stdin = io::stdin();
        for _ in stdin.lock().lines() {
            if line_tx.send(()).is_err() {
                break;
            }
        }
    });

    let mut ready = false;
    let mut listening = false;
    let mut stopping = false;
    let mut last_partial = String::new();

    loop {
        for event in engine.events() {
            match event {
                Event::ModelsLoading => println!("loading model..."),
                Event::ModelsReady => {
                    ready = true;
                    println!("model ready. Press Enter to start listening.");
                }
                Event::Listening => {
                    println!("listening... press Enter to stop.");
                }
                Event::Partial(text) => {
                    if text != last_partial {
                        println!("partial: {text}");
                        last_partial = text;
                    }
                }
                Event::Final(text) => {
                    if text.is_empty() {
                        println!("final:   (nothing recognised)");
                    } else {
                        println!("final:   {text}");
                    }
                    return Ok(());
                }
                Event::Failed(message) => {
                    return Err(message);
                }
            }
        }

        if ready && line_rx.try_recv().is_ok() {
            if !listening && !stopping {
                engine.start().map_err(|error| error.to_string())?;
                listening = true;
            } else if listening && !stopping {
                engine.stop().map_err(|error| error.to_string())?;
                stopping = true;
            }
        }

        std::thread::sleep(Duration::from_millis(25));
    }
}

fn init_logging() {
    let environment = env_logger::Env::default().default_filter_or("warn");
    let mut builder = env_logger::Builder::from_env(environment);
    builder.format_timestamp_millis();
    let _ = builder.try_init();
}

fn print_help() {
    println!(
        "sam-voice — local, offline English speech-to-text\n\n\
         USAGE:\n\
           sam-voice status [--models-root PATH]\n\
           sam-voice setup [--models-root PATH] [--archive PATH]\n\
           sam-voice transcribe <wav> [--threads N] [--models-root PATH]\n\
           sam-voice mic [--threads N] [--models-root PATH]\n\n\
         The model is stored in $XDG_DATA_HOME/sam/models (usually\n\
         ~/.local/share/sam/models). Runtime speech recognition never uses the\n\
         network; `setup` is the only command that downloads anything.\n\n\
         Set RUST_LOG=sam_voice=debug for stage timings."
    );
}
