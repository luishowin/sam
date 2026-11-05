//! `sam-voice setup`: the only code in this crate that uses the network.
//!
//! The download URL and archive contents were verified against the
//! transcribe-rs 0.3.12 README and the published artifact. Nothing here runs
//! while the panel is using speech input.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tar::Archive;

use crate::error::VoiceError;
use crate::model::{
    canonical_model_dir, layout_description, model_status, model_status_for_root, DOWNLOAD_URL,
    MODEL_VARIANT_DIR, TARBALL_SHA256,
};

/// Result of a successful setup run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupOutcome {
    /// The model was already installed; nothing was downloaded.
    AlreadyPresent {
        /// Installed model directory.
        dir: PathBuf,
    },
    /// The model was unpacked and verified.
    Installed {
        /// Installed model directory.
        dir: PathBuf,
    },
}

impl SetupOutcome {
    /// The directory that now contains the model.
    pub fn dir(&self) -> &Path {
        match self {
            SetupOutcome::AlreadyPresent { dir } | SetupOutcome::Installed { dir } => dir,
        }
    }
}

/// Download the Moonshine Small Streaming English model and install it below
/// `models_root`.
pub fn download_and_install(models_root: &Path) -> Result<SetupOutcome, VoiceError> {
    let existing = model_status_for_root(models_root);
    if existing.is_present() {
        return Ok(SetupOutcome::AlreadyPresent {
            dir: existing.dir().to_path_buf(),
        });
    }

    eprintln!(
        "downloading Moonshine Small Streaming English (~100 MB) from\n  {DOWNLOAD_URL}"
    );

    fs::create_dir_all(models_root).map_err(|error| {
        VoiceError::Setup(format!(
            "could not create models root {}: {error}",
            models_root.display()
        ))
    })?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let archive_path = models_root.join(format!(
        ".moonshine-small-streaming-en-{}-{stamp}.tar.gz",
        std::process::id()
    ));

    let result = download_to(DOWNLOAD_URL, &archive_path)
        .and_then(|()| install_archive_inner(&archive_path, models_root, true));

    let _ = fs::remove_file(&archive_path);
    result
}

/// Install an archive that is already on disk. Useful for offline setup and
/// for tests. The archive is checked structurally; the download path also
/// checks the published SHA-256.
pub fn install_archive(archive_path: &Path, models_root: &Path) -> Result<SetupOutcome, VoiceError> {
    install_archive_inner(archive_path, models_root, false)
}

fn install_archive_inner(
    archive_path: &Path,
    models_root: &Path,
    verify_hash: bool,
) -> Result<SetupOutcome, VoiceError> {
    if verify_hash {
        verify_sha256(archive_path, TARBALL_SHA256)?;
    }

    let existing = model_status_for_root(models_root);
    if existing.is_present() {
        return Ok(SetupOutcome::AlreadyPresent {
            dir: existing.dir().to_path_buf(),
        });
    }

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staging = models_root.join(format!(
        ".sam-voice-setup-{}-{stamp}",
        std::process::id()
    ));

    let result = extract_archive(archive_path, &staging)
        .and_then(|()| install_staging(&staging, models_root));

    let _ = fs::remove_dir_all(&staging);
    result
}

fn extract_archive(archive_path: &Path, staging: &Path) -> Result<(), VoiceError> {
    fs::create_dir_all(staging).map_err(|error| {
        VoiceError::Setup(format!(
            "could not create staging directory {}: {error}",
            staging.display()
        ))
    })?;

    let file = File::open(archive_path).map_err(|error| {
        VoiceError::Setup(format!(
            "could not open archive {}: {error}",
            archive_path.display()
        ))
    })?;

    let mut archive = Archive::new(GzDecoder::new(file));
    let entries = archive
        .entries()
        .map_err(|error| VoiceError::Setup(format!("could not read archive: {error}")))?;

    for entry in entries {
        let mut entry = entry
            .map_err(|error| VoiceError::Setup(format!("could not read archive entry: {error}")))?;
        let path = entry
            .path()
            .map_err(|error| VoiceError::Setup(format!("archive entry has no path: {error}")))?
            .into_owned();

        if should_skip(&path) {
            continue;
        }
        if !has_variant_prefix(&path) {
            return Err(VoiceError::Setup(format!(
                "archive entry is outside {MODEL_VARIANT_DIR}/: {}",
                path.display()
            )));
        }

        entry
            .unpack_in(staging)
            .map_err(|error| VoiceError::Setup(format!("could not unpack {}: {error}", path.display())))?;
    }

    Ok(())
}

fn install_staging(staging: &Path, models_root: &Path) -> Result<SetupOutcome, VoiceError> {
    let extracted = staging.join(MODEL_VARIANT_DIR);
    let status = model_status(&extracted);
    if !status.is_present() {
        return Err(VoiceError::Setup(format!(
            "downloaded archive is incomplete; missing: {}\n{}",
            status.missing().join(", "),
            layout_description(models_root)
        )));
    }

    let destination = canonical_model_dir(models_root);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            VoiceError::Setup(format!(
                "could not create model parent {}: {error}",
                parent.display()
            ))
        })?;
    }

    if destination.exists() {
        eprintln!(
            "replacing incomplete model directory {}",
            destination.display()
        );
        fs::remove_dir_all(&destination).map_err(|error| {
            VoiceError::Setup(format!(
                "could not remove incomplete model {}: {error}",
                destination.display()
            ))
        })?;
    }

    fs::rename(&extracted, &destination).map_err(|error| {
        VoiceError::Setup(format!(
            "could not move model into {}: {error}",
            destination.display()
        ))
    })?;

    let installed = model_status(&destination);
    if !installed.is_present() {
        return Err(VoiceError::Setup(format!(
            "installed model failed verification; missing: {}",
            installed.missing().join(", ")
        )));
    }

    Ok(SetupOutcome::Installed { dir: destination })
}

fn should_skip(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(name) => name
            .to_str()
            .map(|name| name.starts_with("._") || name == "__MACOSX")
            .unwrap_or(false),
        _ => false,
    })
}

fn has_variant_prefix(path: &Path) -> bool {
    let mut components = path.components().filter(|component| {
        !matches!(component, Component::CurDir)
    });
    match components.next() {
        Some(Component::Normal(name)) => name == MODEL_VARIANT_DIR,
        _ => false,
    }
}

fn download_to(url: &str, destination: &Path) -> Result<(), VoiceError> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(900)))
        .user_agent("sam-voice-setup/0.1")
        .build();
    let agent: ureq::Agent = config.into();

    let mut response = agent
        .get(url)
        .call()
        .map_err(|error| VoiceError::Setup(format!("download failed: {error}")))?;

    let mut reader = response.body_mut().as_reader();
    let mut output = File::create(destination).map_err(|error| {
        VoiceError::Setup(format!(
            "could not create download file {}: {error}",
            destination.display()
        ))
    })?;

    io::copy(&mut reader, &mut output)
        .map_err(|error| VoiceError::Setup(format!("download interrupted: {error}")))?;

    Ok(())
}

fn verify_sha256(path: &Path, expected: &str) -> Result<(), VoiceError> {
    let mut file = File::open(path)
        .map_err(|error| VoiceError::Setup(format!("could not open download: {error}")))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| VoiceError::Setup(format!("could not read download: {error}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if actual != expected {
        return Err(VoiceError::Setup(format!(
            "checksum mismatch for {} (expected {expected}, got {actual}); the download was discarded",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{model_status_for_root, COMPONENT_NAMES, EXTRA_FILES};
    use crate::testutil::TempDir;

    fn make_archive(path: &Path, include_files: bool) {
        let file = File::create(path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);

        if include_files {
            for name in COMPONENT_NAMES {
                let data = vec![0u8; 16];
                let mut header = tar::Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(
                        &mut header,
                        format!("{MODEL_VARIANT_DIR}/{name}.ort"),
                        data.as_slice(),
                    )
                    .unwrap();
            }
            for name in EXTRA_FILES {
                let data = b"{\"encoder_dim\": 620}".to_vec();
                let mut header = tar::Header::new_gnu();
                header.set_size(data.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(
                        &mut header,
                        format!("{MODEL_VARIANT_DIR}/{name}"),
                        data.as_slice(),
                    )
                    .unwrap();
            }
        }

        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();
    }

    #[test]
    fn installs_a_complete_archive_and_is_idempotent() {
        let temp = TempDir::new("setup-install");
        let archive = temp.path().join("model.tar.gz");
        make_archive(&archive, true);

        let outcome = install_archive(&archive, temp.path()).unwrap();
        assert!(matches!(outcome, SetupOutcome::Installed { .. }));
        assert!(model_status_for_root(temp.path()).is_present());

        let outcome = install_archive(&archive, temp.path()).unwrap();
        assert!(matches!(outcome, SetupOutcome::AlreadyPresent { .. }));
    }

    #[test]
    fn rejects_an_incomplete_archive_without_installing() {
        let temp = TempDir::new("setup-incomplete");
        let archive = temp.path().join("empty.tar.gz");
        make_archive(&archive, false);

        let error = install_archive(&archive, temp.path()).unwrap_err();
        assert!(error.to_string().contains("incomplete"));
        assert!(!canonical_model_dir(temp.path()).exists());
    }

    #[test]
    fn skips_macos_resource_fork_entries() {
        assert!(should_skip(Path::new(
            "moonshine-small-streaming-en/._encoder.ort"
        )));
        assert!(should_skip(Path::new("__MACOSX/foo")));
        assert!(!should_skip(Path::new("moonshine-small-streaming-en/encoder.ort")));
    }
}
