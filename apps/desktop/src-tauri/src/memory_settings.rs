//! Memory safety, chosen in Settings: whether a start first checks the free memory Windows
//! measures, and how much memory a start the user asks for leaves free (the margin).
//!
//! On (the default), an Open, a shell, a harness check or a session read waits up to a few
//! seconds for a fresh measurement and is refused when RAM or commit would fall below its
//! startup allowance plus the margin. Off, nothing waits for or is refused on memory
//! measurements; Shinatama's own count of open workloads still applies. Sessions that are
//! already running are never stopped by this setting. Saved next to the launch settings;
//! an unreadable file leaves the default (on, 128 MiB) in force.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};
use tauri::{State, WebviewWindow};

const FILE: &str = "memory-settings.json";
const MAX_FILE_BYTES: u64 = 4 * 1024;
pub const DEFAULT_MARGIN_MIB: u32 = 128;
pub const MAX_MARGIN_MIB: u32 = 16 * 1024;

static ENABLED: AtomicBool = AtomicBool::new(true);
static MARGIN_MIB: AtomicU32 = AtomicU32::new(DEFAULT_MARGIN_MIB);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MemorySafety {
    pub enabled: bool,
    pub margin_mib: u32,
}
impl Default for MemorySafety {
    fn default() -> Self {
        Self {
            enabled: true,
            margin_mib: DEFAULT_MARGIN_MIB,
        }
    }
}
impl MemorySafety {
    fn validate(self) -> Result<Self, String> {
        if self.margin_mib > MAX_MARGIN_MIB {
            return Err(format!(
                "The memory margin must be between 0 and {MAX_MARGIN_MIB} MiB."
            ));
        }
        Ok(self)
    }
}

/// The setting in force for the next start.
pub fn current() -> MemorySafety {
    MemorySafety {
        enabled: ENABLED.load(Ordering::Acquire),
        margin_mib: MARGIN_MIB.load(Ordering::Acquire),
    }
}
fn set_current(value: MemorySafety) {
    MARGIN_MIB.store(value.margin_mib, Ordering::Release);
    ENABLED.store(value.enabled, Ordering::Release);
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Stored {
    version: u8,
    enabled: bool,
    margin_mib: u32,
}

pub struct Controller {
    path: Result<PathBuf, String>,
    lock: Mutex<()>,
}
impl Controller {
    pub fn start(directory: Result<PathBuf, String>) -> Arc<Self> {
        let controller = Arc::new(Self {
            path: directory.map(|directory| directory.join(FILE)),
            lock: Mutex::new(()),
        });
        let _ = controller.reload();
        controller
    }
    fn path(&self) -> Result<&Path, String> {
        self.path.as_deref().map_err(Clone::clone)
    }
    fn read(&self) -> Result<MemorySafety, String> {
        let path = self.path()?;
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(MemorySafety::default());
            }
            Err(_) => return Err("The memory settings could not be read.".into()),
        };
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
            return Err("The memory settings file is not usable; it was kept as it is.".into());
        }
        let text = fs::read_to_string(path)
            .map_err(|_| "The memory settings could not be read.".to_owned())?;
        let stored: Stored = serde_json::from_str(&text)
            .map_err(|_| "The memory settings file is damaged; it was kept as it is.".to_owned())?;
        if stored.version != 1 {
            return Err("The memory settings were saved by a newer Shinatama.".into());
        }
        MemorySafety {
            enabled: stored.enabled,
            margin_mib: stored.margin_mib,
        }
        .validate()
    }
    fn write(&self, value: MemorySafety) -> Result<(), String> {
        let path = self.path()?;
        let directory = path.parent().ok_or("The settings folder is unavailable.")?;
        fs::create_dir_all(directory)
            .map_err(|_| "The settings folder could not be created.".to_owned())?;
        let text = serde_json::to_vec_pretty(&Stored {
            version: 1,
            enabled: value.enabled,
            margin_mib: value.margin_mib,
        })
        .map_err(|_| "The memory settings could not be written.".to_owned())?;
        let temporary = path.with_extension("json.tmp");
        let result = (|| {
            let mut file = fs::File::create(&temporary)?;
            file.write_all(&text)?;
            file.sync_all()?;
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
            return Err("The memory settings could not be saved; the old ones were kept.".into());
        }
        Ok(())
    }
    /// The saved setting, or the default (memory safety on) when it cannot be read.
    fn reload(&self) -> Result<MemorySafety, String> {
        match self.read() {
            Ok(value) => {
                set_current(value);
                Ok(value)
            }
            Err(error) => {
                set_current(MemorySafety::default());
                Err(error)
            }
        }
    }
    fn load(&self) -> Result<MemorySafety, String> {
        let _guard = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        self.reload()
    }
    fn save(&self, value: MemorySafety) -> Result<MemorySafety, String> {
        let _guard = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        let value = value.validate()?;
        self.write(value)?;
        set_current(value);
        Ok(value)
    }
}

#[tauri::command]
pub async fn memory_settings_load(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
) -> Result<MemorySafety, String> {
    crate::local(&window)?;
    let controller = controller.inner().clone();
    tauri::async_runtime::spawn_blocking(move || controller.load())
        .await
        .map_err(|_| "The settings worker did not return.".to_owned())?
}
#[tauri::command]
pub async fn memory_settings_save(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: MemorySafety,
) -> Result<MemorySafety, String> {
    crate::local(&window)?;
    let controller = controller.inner().clone();
    tauri::async_runtime::spawn_blocking(move || controller.save(request))
        .await
        .map_err(|_| "The settings worker did not return.".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::MutexGuard;

    /// The setting is process-wide: tests that change it run one at a time.
    fn serial() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }
    fn controller(name: &str) -> (Arc<Controller>, PathBuf) {
        let directory = std::env::temp_dir().join(format!(
            "shinatama-memory-settings-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&directory).unwrap();
        let controller = Arc::new(Controller {
            path: Ok(directory.join(FILE)),
            lock: Mutex::new(()),
        });
        (controller, directory)
    }

    #[test]
    fn a_fresh_install_checks_memory_with_the_default_margin() {
        let _serial = serial();
        let (controller, _dir) = controller("fresh");
        assert_eq!(controller.load().unwrap(), MemorySafety::default());
        assert!(current().enabled);
        assert_eq!(current().margin_mib, DEFAULT_MARGIN_MIB);
    }

    #[test]
    fn a_saved_choice_is_in_force_and_survives_a_reload() {
        let _serial = serial();
        let (controller, dir) = controller("saved");
        let chosen = MemorySafety {
            enabled: false,
            margin_mib: 512,
        };
        assert_eq!(controller.save(chosen).unwrap(), chosen);
        assert_eq!(current(), chosen);
        set_current(MemorySafety::default());
        assert_eq!(controller.load().unwrap(), chosen);
        let disk = fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(disk.contains("\"marginMib\": 512") && disk.contains("\"enabled\": false"));
        set_current(MemorySafety::default());
    }

    #[test]
    fn an_out_of_range_margin_is_refused_and_leaves_the_saved_choice() {
        let _serial = serial();
        let (controller, _dir) = controller("range");
        let error = controller
            .save(MemorySafety {
                enabled: true,
                margin_mib: MAX_MARGIN_MIB + 1,
            })
            .unwrap_err();
        assert!(error.contains("between 0 and"));
        assert_eq!(current(), MemorySafety::default());
    }

    #[test]
    fn a_damaged_file_falls_back_to_memory_safety_on() {
        let _serial = serial();
        let (controller, dir) = controller("damaged");
        set_current(MemorySafety {
            enabled: false,
            margin_mib: 0,
        });
        fs::write(dir.join(FILE), b"{ nope").unwrap();
        assert!(controller.load().unwrap_err().contains("damaged"));
        assert_eq!(current(), MemorySafety::default());
    }
}
