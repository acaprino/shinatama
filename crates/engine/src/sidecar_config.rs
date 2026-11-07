//! C12 private filesystem preparation only. No subprocess, inherited configuration,
//! credentials, CA contents, user database or certificate store is read. Existing
//! configuration is preserved byte-for-byte and remains semantically unverified.
use serde::Serialize;
use sha2::{Digest, Sha256};
use shinatama_state::location::StorageLocation;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::num::NonZeroU16;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    PrivateDirectory,
    InvalidPath,
    Configuration,
    WriteUncertain,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigDisposition {
    Created,
    ExistingPreserved,
}
#[derive(Clone, Debug, Serialize)]
pub struct ConfigSummary {
    pub disposition: ConfigDisposition,
    pub owner_only_verified: bool,
    pub written_baseline_sha256: Option<String>,
    pub expected_storage_schema: u32,
    pub requires_incremental_ccr_source: bool,
    pub intended_effort_routing: bool,
    pub effective_configuration_verified: bool,
    pub runtime_qualified: bool,
}
pub struct PreparedSidecarConfig {
    root: StorageLocation,
    _locations: Vec<StorageLocation>,
    config: PathBuf,
    summary: ConfigSummary,
}
impl PreparedSidecarConfig {
    pub fn root(&self) -> &StorageLocation {
        &self.root
    }
    pub fn configuration_file(&self) -> &Path {
        &self.config
    }
    pub fn summary(&self) -> &ConfigSummary {
        &self.summary
    }
}
impl std::fmt::Debug for PreparedSidecarConfig {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("PreparedSidecarConfig")
            .field("summary", &self.summary)
            .finish_non_exhaustive()
    }
}

/// Blocking filesystem operations belong on a single bounded preparation worker.
/// Every directory is private at creation through C01, with grants held alive.
/// The only file write uses create_new; existing content is never opened/read.
pub fn prepare_private_sidecar(
    base: &StorageLocation,
    port: NonZeroU16,
) -> Result<PreparedSidecarConfig, ConfigError> {
    base.verify_private_directory()
        .map_err(|_| ConfigError::PrivateDirectory)?;
    let root = base
        .resolve_related_directory(&base.directory().join("sidecar"))
        .map_err(|_| ConfigError::PrivateDirectory)?;
    root.prepare_private_directory()
        .map_err(|_| ConfigError::PrivateDirectory)?;
    let mut locations = vec![base.clone(), root.clone()];
    for relative in [
        "home",
        "config",
        "data",
        "cache",
        "state",
        "temp",
        "cache/node-compile",
        "data/sessions",
        "data/ca",
        "state/dumps",
        "home/AppData/Roaming",
        "home/AppData/Local",
    ] {
        let location = root
            .resolve_related_directory(&root.directory().join(relative))
            .map_err(|_| ConfigError::PrivateDirectory)?;
        location
            .prepare_private_directory()
            .map_err(|_| ConfigError::PrivateDirectory)?;
        locations.push(location);
    }
    let config_directory = root
        .resolve_related_directory(&root.directory().join("config"))
        .map_err(|_| ConfigError::PrivateDirectory)?;
    let config = config_directory.directory().join("billion-context.json");
    let bytes = baseline(root.directory(), port)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    let disposition = match options.open(&config) {
        Ok(mut file) => {
            // A failed/partial write is retained for explicit repair. Never
            // overwrite or delete a configuration after uncertainty or restart.
            file.write_all(&bytes)
                .map_err(|_| ConfigError::WriteUncertain)?;
            file.sync_all().map_err(|_| ConfigError::WriteUncertain)?;
            ConfigDisposition::Created
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            ConfigDisposition::ExistingPreserved
        }
        Err(_) => return Err(ConfigError::Configuration),
    };
    if disposition == ConfigDisposition::Created {
        // New files inherit the private directory ACL. C01 additionally requires
        // an explicit protected file DACL; only our create_new file is secured.
        config_directory
            .secure_private_file(&config)
            .map_err(|_| ConfigError::Configuration)?;
    }
    config_directory
        .verify_private_file(&config)
        .map_err(|_| ConfigError::Configuration)?;
    locations.push(config_directory);
    let summary = ConfigSummary {
        disposition,
        owner_only_verified: true,
        written_baseline_sha256: (disposition == ConfigDisposition::Created)
            .then(|| format!("{:x}", Sha256::digest(&bytes))),
        expected_storage_schema: 9,
        requires_incremental_ccr_source: true,
        intended_effort_routing: false,
        effective_configuration_verified: false,
        runtime_qualified: false,
    };
    Ok(PreparedSidecarConfig {
        root,
        _locations: locations,
        config,
        summary,
    })
}
fn baseline(root: &Path, port: NonZeroU16) -> Result<Vec<u8>, ConfigError> {
    let logfile = root.join("state/bili.log");
    let logfile = logfile.to_str().ok_or(ConfigError::InvalidPath)?;
    if !root.is_absolute() || logfile.len() > 4096 || logfile.as_bytes().contains(&0) {
        return Err(ConfigError::InvalidPath);
    }
    // These are actual canonical FileConfig/CompressSettings keys:
    // src/config.ts, musashi-terminal/adaptive-pressure.ts, and kernel's
    // output-steering.ts. No local implementation of schema/CCR is introduced.
    let value = serde_json::json!({
        "host": "127.0.0.1", "port": port.get(), "log": true, "debug": false, "logFile": logfile,
        "autoUpdate": false, "autoRestartOnUpdate": false, "advisoryCheck": false, "releaseNotesCheck": false,
        "storage": { "unified": "full", "backend": "sqlite", "sqliteAutoDownload": false },
        "promptRewrite": { "mode": "auto" },
        "compress": {
            "tiers": true,
            "absorb": { "enabled": true, "minToolTokens": 4000 },
            "ccr": { "enabled": true, "minToolTokens": 4000 },
            // targetRatio and growthWindow are validated by the canonical parser but
            // consumed by no code path; omitting them keeps its 0.75/20 defaults
            // (identical behavior) without presenting them as Shinatama controls.
            "adaptivePressure": { "enabled": true, "maxTokens": 160000 },
            "outputSteering": { "enabled": true, "verbosityLevel": 2, "effortRouting": false }
        }
    });
    let mut bytes = serde_json::to_vec_pretty(&value).map_err(|_| ConfigError::Configuration)?;
    bytes.push(b'\n');
    if bytes.len() > 16 * 1024 {
        return Err(ConfigError::Configuration);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn baseline_uses_canonical_storage_and_compression_keys_with_effort_locked_off() {
        let root = if cfg!(windows) {
            Path::new("C:\\synthetic-sidecar")
        } else {
            Path::new("/synthetic-sidecar")
        };
        let value: serde_json::Value =
            serde_json::from_slice(&baseline(root, NonZeroU16::new(8787).unwrap()).unwrap())
                .unwrap();
        assert_eq!(value["host"], "127.0.0.1");
        assert_eq!(value["port"], 8787);
        assert_eq!(value["storage"]["unified"], "full");
        assert_eq!(value["storage"]["sqliteAutoDownload"], false);
        assert_eq!(value["compress"]["ccr"]["minToolTokens"], 4000);
        assert_eq!(value["compress"]["outputSteering"]["effortRouting"], false);
        assert_eq!(value["compress"]["adaptivePressure"]["enabled"], true);
        assert_eq!(value["compress"]["adaptivePressure"]["maxTokens"], 160000);
        // C12.5: the two unconsumed canonical fields are not product controls.
        assert!(
            value["compress"]["adaptivePressure"]
                .get("targetRatio")
                .is_none()
        );
        assert!(
            value["compress"]["adaptivePressure"]
                .get("growthWindow")
                .is_none()
        );
        assert_eq!(value["promptRewrite"]["mode"], "auto");
        assert!(value.get("providers").is_none());
        assert!(value.get("schemaVersion").is_none());
    }
    #[cfg(windows)]
    #[test]
    fn c01_private_preparation_preserves_existing_synthetic_configuration_bytes() {
        use shinatama_state::location::LocationRequest;
        use std::fs;
        let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
        let root = temp.join(format!("shinatama-sidecar-config-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let result = std::panic::catch_unwind(|| {
            let location = StorageLocation::resolve(LocationRequest {
                local_data_directory: root.join("platform"),
                home_override: Some(root.join("private")),
                project_roots: Vec::new(),
                synchronized_roots: Vec::new(),
            })
            .unwrap();
            location.prepare_private_directory().unwrap();
            let prepared =
                prepare_private_sidecar(&location, NonZeroU16::new(8787).unwrap()).unwrap();
            assert_eq!(prepared.summary().disposition, ConfigDisposition::Created);
            assert!(prepared.summary().written_baseline_sha256.is_some());
            let config = prepared.configuration_file().to_path_buf();
            let original = b"synthetic-existing-configuration-must-not-be-read-or-replaced";
            fs::write(&config, original).unwrap();
            let again = prepare_private_sidecar(&location, NonZeroU16::new(9898).unwrap()).unwrap();
            assert_eq!(
                again.summary().disposition,
                ConfigDisposition::ExistingPreserved
            );
            assert_eq!(again.summary().written_baseline_sha256, None);
            assert!(!again.summary().effective_configuration_verified);
            assert_eq!(fs::read(&config).unwrap(), original);
        });
        assert_eq!(root.parent(), Some(temp.as_path()));
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("shinatama-sidecar-config-")
        );
        let _ = fs::remove_dir_all(&root);
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }
}
