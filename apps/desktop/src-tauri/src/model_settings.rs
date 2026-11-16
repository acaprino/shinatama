//! One encrypted provider store shared by every harness. Keys never return over IPC.
use crate::secret_store;
use serde::{Deserialize, Serialize};
use shinatama_engine::model_config::{self, Assignments, Configuration, Connection, Snapshot};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{State, WebviewWindow};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Stored {
    version: u8,
    revision: u64,
    configuration: Configuration,
    keys: BTreeMap<String, String>,
}
impl Stored {
    fn initial() -> Self {
        Self {
            version: 1,
            ..Self::default()
        }
    }
    fn snapshot(&self) -> Result<Snapshot, String> {
        validate(&self.configuration)?;
        let mut keys = BTreeMap::new();
        for (id, blob) in &self.keys {
            if !self.configuration.connections.iter().any(|c| &c.id == id) {
                return Err("The model store has an orphaned key.".into());
            }
            keys.insert(
                id.clone(),
                secret_store::unprotect(blob).map_err(str::to_owned)?,
            );
        }
        Ok(Snapshot {
            configuration: self.configuration.clone(),
            keys,
        })
    }
    fn view(&self) -> View {
        View {
            revision: self.revision,
            assignments: self.configuration.assignments.clone(),
            connections: self
                .configuration
                .connections
                .iter()
                .map(|c| ConnectionView {
                    connection: c.clone(),
                    has_api_key: self.keys.contains_key(&c.id),
                })
                .collect(),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    revision: u64,
    connections: Vec<ConnectionView>,
    assignments: Assignments,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionView {
    connection: Connection,
    has_api_key: bool,
}
#[derive(Deserialize)]
#[serde(
    tag = "op",
    content = "value",
    rename_all = "camelCase",
    deny_unknown_fields
)]
enum KeyEdit {
    Keep,
    Set(String),
    Clear,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Save {
    revision: u64,
    connections: Vec<ConnectionSave>,
    assignments: Assignments,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ConnectionSave {
    connection: Connection,
    api_key: KeyEdit,
}

fn validate(configuration: &Configuration) -> Result<(), String> {
    configuration.validate()?;
    for c in &configuration.connections {
        let url = url::Url::parse(&c.base_url).map_err(|_| "The base URL is not valid.")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || c.base_url.contains('\\')
        {
            return Err(
                "Use an HTTP(S) base URL without credentials, query parameters or fragments."
                    .into(),
            );
        }
    }
    Ok(())
}
fn merge(previous: Stored, save: Save) -> Result<Stored, String> {
    if save.revision != previous.revision {
        return Err("Model settings changed in another window. Reload before saving.".into());
    }
    let configuration = Configuration {
        connections: save
            .connections
            .iter()
            .map(|c| c.connection.clone())
            .collect(),
        assignments: save.assignments,
    };
    validate(&configuration)?;
    let mut keys = BTreeMap::new();
    for item in save.connections {
        let blob = match item.api_key {
            KeyEdit::Keep => previous.keys.get(&item.connection.id).cloned(),
            KeyEdit::Clear => None,
            KeyEdit::Set(key) => {
                if key.is_empty()
                    || key.len() > 4096
                    || key.chars().any(|c| c.is_whitespace() || c.is_control())
                {
                    return Err(
                        "An API key must be 1–4096 bytes without spaces or control characters."
                            .into(),
                    );
                }
                Some(secret_store::protect(&key).map_err(str::to_owned)?)
            }
        };
        if let Some(blob) = blob {
            keys.insert(item.connection.id, blob);
        }
    }
    Ok(Stored {
        version: 1,
        revision: previous
            .revision
            .checked_add(1)
            .ok_or("The model store revision is exhausted.")?,
        configuration,
        keys,
    })
}

pub struct Controller {
    path: Result<PathBuf, String>,
    lock: Mutex<()>,
}
impl Controller {
    pub fn start(directory: Result<PathBuf, String>) -> Arc<Self> {
        let controller = Arc::new(Self {
            path: directory.map(|p| p.join("model-settings.json")),
            lock: Mutex::new(()),
        });
        let _ = controller.load();
        controller
    }
    fn path(&self) -> Result<&Path, String> {
        self.path.as_deref().map_err(Clone::clone)
    }
    fn read(&self) -> Result<Stored, String> {
        let file = match fs::File::open(self.path()?) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Stored::initial()),
            Err(_) => return Err("The model settings could not be read.".into()),
        };
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "The model settings could not be read.")?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err("The model settings file is too large.".into());
        }
        let stored: Stored = serde_json::from_slice(&bytes)
            .map_err(|_| "The model settings file is damaged; it was kept unchanged.")?;
        if stored.version != 1 {
            return Err("The model settings were saved by a newer Shinatama.".into());
        }
        Ok(stored)
    }
    fn write(&self, stored: &Stored) -> Result<(), String> {
        let path = self.path()?;
        fs::create_dir_all(path.parent().ok_or("The settings folder is unavailable.")?)
            .map_err(|_| "The settings folder could not be created.")?;
        let bytes = serde_json::to_vec_pretty(stored)
            .map_err(|_| "The model settings could not be encoded.")?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err("The model settings are too large.".into());
        }
        let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> std::io::Result<()> {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
            return Err("The model settings could not be saved; the old ones were kept.".into());
        }
        Ok(())
    }
    fn load(&self) -> Result<View, String> {
        let _guard = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        let result = self
            .read()
            .and_then(|s| s.snapshot().map(|snapshot| (s, snapshot)));
        match result {
            Ok((stored, snapshot)) => {
                model_config::set_current(Ok(snapshot));
                Ok(stored.view())
            }
            Err(error) => {
                model_config::set_current(Err(
                    "Model settings could not be loaded. Open Settings → Models and reload.".into(),
                ));
                Err(error)
            }
        }
    }
    fn save(&self, save: Save) -> Result<View, String> {
        let _guard = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        let previous = self.read()?;
        // Confirm existing keys are usable before preserving them or accepting an edit.
        previous.snapshot()?;
        let next = merge(previous, save)?;
        let snapshot = next.snapshot()?;
        self.write(&next)?;
        model_config::set_current(Ok(snapshot));
        Ok(next.view())
    }
}
#[tauri::command]
pub async fn model_settings_load(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
) -> Result<View, String> {
    crate::local(&window)?;
    let controller = controller.inner().clone();
    tauri::async_runtime::spawn_blocking(move || controller.load())
        .await
        .map_err(|_| "The model settings worker did not return.")?
}
#[tauri::command]
pub async fn model_settings_save(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: Save,
) -> Result<View, String> {
    crate::local(&window)?;
    let controller = controller.inner().clone();
    tauri::async_runtime::spawn_blocking(move || controller.save(request))
        .await
        .map_err(|_| "The model settings worker did not return.")?
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use serde_json::json;
    fn save(revision: u64, key: serde_json::Value) -> Save {
        serde_json::from_value(json!({"revision":revision, "connections":[{"connection": {
            "id":"test", "name":"Test", "api":"openai-completions", "baseUrl":"http://localhost:1234/v1",
            "models":[{"id":"custom/model", "contextWindow":32000, "maxTokens":4000, "reasoning":false, "vision":false}]
        }, "apiKey":key}], "assignments":{"pi":{"connectionId":"test","modelId":"custom/model"},"opencode":null,"hermes":null}})).unwrap()
    }
    #[test]
    fn keys_round_trip_without_leaking_and_edits_preserve_replace_clear() {
        let stored = merge(
            Stored::initial(),
            save(0, json!({"op":"set","value":"synthetic-key"})),
        )
        .unwrap();
        assert!(
            !serde_json::to_string(&stored)
                .unwrap()
                .contains("synthetic-key")
        );
        assert!(
            !serde_json::to_string(&stored.view())
                .unwrap()
                .contains("synthetic-key")
        );
        assert_eq!(stored.snapshot().unwrap().keys["test"], "synthetic-key");
        let stored = merge(stored, save(1, json!({"op":"keep"}))).unwrap();
        assert_eq!(stored.snapshot().unwrap().keys["test"], "synthetic-key");
        let stored = merge(stored, save(2, json!({"op":"set","value":"replacement"}))).unwrap();
        assert_eq!(stored.snapshot().unwrap().keys["test"], "replacement");
        let stored = merge(stored, save(3, json!({"op":"clear"}))).unwrap();
        assert!(stored.keys.is_empty());
    }
    #[test]
    fn persistence_survives_reload_and_rejects_stale_or_corrupt_edits() {
        let dir = std::env::temp_dir().join(format!("shinatama-models-{}", uuid::Uuid::new_v4()));
        let c = Controller {
            path: Ok(dir.join("model-settings.json")),
            lock: Mutex::new(()),
        };
        c.save(save(0, json!({"op":"set","value":"synthetic-key"})))
            .unwrap();
        assert_eq!(
            c.read().unwrap().snapshot().unwrap().keys["test"],
            "synthetic-key"
        );
        assert!(c.save(save(0, json!({"op":"clear"}))).is_err());
        fs::write(c.path().unwrap(), "broken").unwrap();
        assert!(c.save(save(1, json!({"op":"keep"}))).is_err());
        assert_eq!(fs::read_to_string(c.path().unwrap()).unwrap(), "broken");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn invalid_endpoints_and_key_values_are_rejected_without_echoing_secrets() {
        for url in [
            "https://user:secret@example.com",
            "https://example.com?key=secret",
            "https://example.com/#secret",
            "file:///x",
            "https://",
        ] {
            let mut request = save(0, json!({"op":"clear"}));
            request.connections[0].connection.base_url = url.into();
            assert!(merge(Stored::initial(), request).is_err(), "{url}");
        }
        let error = merge(
            Stored::initial(),
            save(0, json!({"op":"set","value":"secret\nvalue"})),
        )
        .err()
        .unwrap();
        assert!(!error.contains("secret\nvalue"));
    }
}
