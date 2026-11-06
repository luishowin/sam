//! Model locations and verification.
//!
//! The canonical installation is:
//!
//! ```text
//! ~/.local/share/sam/models/moonshine-streaming/moonshine-small-streaming-en/
//! ├── frontend.ort
//! ├── encoder.ort
//! ├── adapter.ort
//! ├── cross_kv.ort
//! ├── decoder_kv.ort
//! ├── streaming_config.json
//! └── tokenizer.bin
//! ```
//!
//! transcribe-rs 0.3.12 also accepts `.onnx` files in place of `.ort`, so a
//! directory containing the five components with either suffix is considered
//! installed. The `tokenizer.bin` name is what 0.3.12 actually reads; older
//! documentation that says `tokenizer.json` does not apply to this version.

use std::env;
use std::path::{Path, PathBuf};

use crate::VoiceError;

/// Directory under the models root that holds Moonshine streaming models.
pub const MODEL_FAMILY_DIR: &str = "moonshine-streaming";
/// The only model this build supports.
pub const MODEL_VARIANT_DIR: &str = "moonshine-small-streaming-en";
/// Names of the five ONNX Runtime components, without an extension.
pub const COMPONENT_NAMES: &[&str] = &["frontend", "encoder", "adapter", "cross_kv", "decoder_kv"];
/// Files that are not ONNX components.
pub const EXTRA_FILES: &[&str] = &["streaming_config.json", "tokenizer.bin"];

/// Verified download URL from the transcribe-rs README (Moonshine table).
pub const DOWNLOAD_URL: &str =
    "https://blob.handy.computer/moonshine-small-streaming-en.tar.gz";

/// SHA-256 of the small streaming English archive.
///
/// Measured from the artifact published at [`DOWNLOAD_URL`]. Setup verifies it
/// before unpacking, so a changed or truncated download is never installed.
pub const TARBALL_SHA256: &str =
    "dbb3e1c1832bd88a4ac712f7449a136cc2c9a18c5fe33a12ed1b7cb1cfe9cdd5";

/// `$XDG_DATA_HOME/sam/models`, or `~/.local/share/sam/models`.
pub fn default_models_root() -> PathBuf {
    if let Some(data_home) = env::var_os("XDG_DATA_HOME") {
        let data_home = PathBuf::from(data_home);
        if !data_home.as_os_str().is_empty() {
            return data_home.join("sam").join("models");
        }
    }
    match env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".local/share/sam/models"),
        None => PathBuf::from(".local/share/sam/models"),
    }
}

/// The canonical model directory below a models root.
pub fn canonical_model_dir(models_root: &Path) -> PathBuf {
    models_root.join(MODEL_FAMILY_DIR).join(MODEL_VARIANT_DIR)
}

/// Whether all files below `dir` exist and are non-empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelStatus {
    /// Every required file is present.
    Present {
        /// The directory that was checked.
        dir: PathBuf,
    },
    /// At least one required file is absent or empty.
    Missing {
        /// The directory that was checked.
        dir: PathBuf,
        /// Human-readable names of the missing files.
        missing: Vec<String>,
    },
}

impl ModelStatus {
    /// Whether the model can be loaded.
    pub fn is_present(&self) -> bool {
        matches!(self, ModelStatus::Present { .. })
    }

    /// The directory that was checked.
    pub fn dir(&self) -> &Path {
        match self {
            ModelStatus::Present { dir } | ModelStatus::Missing { dir, .. } => dir,
        }
    }

    /// Missing file descriptions. Empty when the model is present.
    pub fn missing(&self) -> &[String] {
        match self {
            ModelStatus::Present { .. } => &[],
            ModelStatus::Missing { missing, .. } => missing,
        }
    }
}

/// Check a model directory. This does not load ONNX Runtime.
pub fn model_status(dir: &Path) -> ModelStatus {
    let mut missing = Vec::new();

    for name in COMPONENT_NAMES {
        let ort = dir.join(format!("{name}.ort"));
        let onnx = dir.join(format!("{name}.onnx"));
        if !is_non_empty_file(&ort) && !is_non_empty_file(&onnx) {
            missing.push(format!("{name}.ort (or {name}.onnx)"));
        }
    }

    for name in EXTRA_FILES {
        if !is_non_empty_file(&dir.join(name)) {
            missing.push((*name).to_string());
        }
    }

    if missing.is_empty() {
        ModelStatus::Present {
            dir: dir.to_path_buf(),
        }
    } else {
        ModelStatus::Missing {
            dir: dir.to_path_buf(),
            missing,
        }
    }
}

/// Find an installed model below a models root.
///
/// Both the documented nested layout and a flat layout (files directly in
/// `moonshine-streaming/`) are accepted. If nothing is installed, the canonical
/// nested path is returned so the error message points at where setup will put
/// the files.
pub fn discover_model_dir(models_root: &Path) -> PathBuf {
    let candidates = [
        canonical_model_dir(models_root),
        models_root.join(MODEL_FAMILY_DIR),
        models_root.join(MODEL_VARIANT_DIR),
        models_root.to_path_buf(),
    ];

    for candidate in &candidates {
        if model_status(candidate).is_present() {
            return candidate.clone();
        }
    }

    canonical_model_dir(models_root)
}

/// Resolve the directory selected in `voice.model`.
///
/// The setting may name the models root, the `moonshine-streaming` family
/// directory, or the model directory itself. The first complete directory wins;
/// when nothing is installed, the selection itself is returned so the error
/// names the path the user configured.
pub fn resolve_model_dir(selection: &Path) -> PathBuf {
    let candidates = [
        selection.to_path_buf(),
        selection.join(MODEL_VARIANT_DIR),
        selection.join(MODEL_FAMILY_DIR).join(MODEL_VARIANT_DIR),
        selection.join(MODEL_FAMILY_DIR),
    ];

    for candidate in candidates {
        if model_status(&candidate).is_present() {
            return candidate;
        }
    }

    selection.to_path_buf()
}

/// Check the installed model below a models root, using the same discovery rules.
pub fn model_status_for_root(models_root: &Path) -> ModelStatus {
    model_status(&discover_model_dir(models_root))
}

/// The directory the setup instructions should name for a selection.
fn expected_model_dir(selection: &Path) -> PathBuf {
    match selection.file_name().and_then(|name| name.to_str()) {
        Some(MODEL_VARIANT_DIR) => selection.to_path_buf(),
        Some(MODEL_FAMILY_DIR) => selection.join(MODEL_VARIANT_DIR),
        _ => canonical_model_dir(selection),
    }
}

/// A short, copy-pasteable description of the expected layout.
pub fn layout_description(models_root: &Path) -> String {
    let dir = expected_model_dir(models_root);
    format!(
        "expected model layout:\n{}\n├── frontend.ort\n├── encoder.ort\n├── adapter.ort\n├── cross_kv.ort\n├── decoder_kv.ort\n├── streaming_config.json\n└── tokenizer.bin\n\nrun `sam-voice setup` to download it from {DOWNLOAD_URL}",
        dir.display()
    )
}

/// Build the error used by the worker and the CLI when the model is absent.
pub fn missing_model_error(status: &ModelStatus) -> VoiceError {
    match status {
        ModelStatus::Present { .. } => VoiceError::ModelLoad("model unexpectedly absent".into()),
        ModelStatus::Missing { dir, missing } => VoiceError::ModelsMissing {
            dir: dir.clone(),
            missing: missing.clone(),
        },
    }
}

fn is_non_empty_file(path: &Path) -> bool {
    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn install_fake_model(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        for name in COMPONENT_NAMES {
            std::fs::write(dir.join(format!("{name}.ort")), [0u8; 8]).unwrap();
        }
        std::fs::write(dir.join("streaming_config.json"), b"{\"encoder_dim\":1}").unwrap();
        std::fs::write(dir.join("tokenizer.bin"), [0u8; 8]).unwrap();
    }

    #[test]
    fn status_reports_present_for_complete_model() {
        let temp = TempDir::new("model-present");
        let dir = canonical_model_dir(temp.path());
        install_fake_model(&dir);

        let status = model_status(&dir);
        assert!(status.is_present());
        assert!(status.missing().is_empty());
    }

    #[test]
    fn status_lists_missing_and_empty_files() {
        let temp = TempDir::new("model-missing");
        let dir = canonical_model_dir(temp.path());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("encoder.ort"), []).unwrap();

        let status = model_status(&dir);
        assert!(!status.is_present());
        assert!(status.missing().iter().any(|name| name.starts_with("frontend")));
        assert!(status.missing().iter().any(|name| name == "encoder.ort (or encoder.onnx)"));
        assert!(status.missing().iter().any(|name| name == "tokenizer.bin"));
    }

    #[test]
    fn discover_accepts_nested_flat_and_variant_layouts() {
        let temp = TempDir::new("model-discover");
        let root = temp.path();

        // Nothing installed: canonical path.
        assert_eq!(discover_model_dir(root), canonical_model_dir(root));

        // Flat files in moonshine-streaming/.
        let flat = root.join(MODEL_FAMILY_DIR);
        install_fake_model(&flat);
        assert_eq!(discover_model_dir(root), flat);

        // Nested layout wins over the flat one.
        let nested = canonical_model_dir(root);
        install_fake_model(&nested);
        assert_eq!(discover_model_dir(root), nested);

        // A root that is itself the model directory.
        assert_eq!(discover_model_dir(&nested), nested);
    }

    #[test]
    fn missing_model_error_has_actionable_message() {
        let temp = TempDir::new("model-error");
        let status = model_status(&temp.path().join("nowhere"));
        let error = missing_model_error(&status);
        let message = error.to_string();
        assert!(message.contains("Moonshine Small Streaming"));
        assert!(message.contains("sam-voice setup"));
    }

    #[test]
    fn resolve_accepts_root_family_and_model_selection() {
        let temp = TempDir::new("model-resolve");
        let root = temp.path();
        let nested = canonical_model_dir(root);
        install_fake_model(&nested);

        assert_eq!(resolve_model_dir(root), nested);
        assert_eq!(resolve_model_dir(&root.join(MODEL_FAMILY_DIR)), nested);
        assert_eq!(resolve_model_dir(&nested), nested);

        let empty = root.join("not-installed");
        assert_eq!(resolve_model_dir(&empty), empty);
    }

    #[test]
    fn layout_names_the_selected_directory() {
        let temp = TempDir::new("model-layout");
        let root = temp.path();
        assert!(layout_description(root).contains(
            &canonical_model_dir(root).display().to_string()
        ));
        assert!(
            layout_description(&root.join(MODEL_FAMILY_DIR)).contains(
                &root
                    .join(MODEL_FAMILY_DIR)
                    .join(MODEL_VARIANT_DIR)
                    .display()
                    .to_string()
            )
        );
    }
}
