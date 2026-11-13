//! The user's launch settings: full access, a GitHub sign-in and extra
//! variables that every real agent session receives. Secrets are protected with
//! the Windows user's DPAPI key and never go back to the page: the page learns
//! only that a secret exists and can replace or clear it.
//!
//! The saved choice becomes the engine's launch policy at startup and on every
//! save; sessions that are already running keep the environment they started with.
use crate::secret_store;
use serde::{Deserialize, Serialize};
use shinatama_engine::launch_policy::{self, LaunchPolicy};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{State, WebviewWindow};

const FILE: &str = "launch-settings.json";
const MAX_FILE_BYTES: u64 = 256 * 1024;
const MAX_VARIABLES: usize = 32;
const MAX_VALUE_BYTES: usize = 4096;
const MAX_TOKEN_BYTES: usize = 255;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Stored {
    version: u8,
    full_access: bool,
    github_username: String,
    /// DPAPI blob, lowercase hex.
    github_token: Option<String>,
    variables: Vec<StoredVariable>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct StoredVariable {
    name: String,
    secret: bool,
    /// Plain text, or a DPAPI blob in lowercase hex when `secret`.
    value: String,
}
impl Stored {
    /// What a fresh install starts with: agents run without their own protections.
    fn initial() -> Self {
        Self {
            version: 1,
            full_access: true,
            github_username: String::new(),
            github_token: None,
            variables: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct View {
    full_access: bool,
    github_username: String,
    has_github_token: bool,
    variables: Vec<ViewVariable>,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ViewVariable {
    name: String,
    secret: bool,
    /// Absent for a secret: it never leaves the backend.
    value: Option<String>,
    has_value: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", content = "value", rename_all = "camelCase")]
enum SecretEdit {
    Keep,
    Set(String),
    Clear,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Save {
    full_access: bool,
    github_username: String,
    github_token: SecretEdit,
    variables: Vec<SaveVariable>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SaveVariable {
    name: String,
    secret: bool,
    value: SecretEdit,
}

fn view(stored: &Stored) -> View {
    View {
        full_access: stored.full_access,
        github_username: stored.github_username.clone(),
        has_github_token: stored.github_token.is_some(),
        variables: stored
            .variables
            .iter()
            .map(|variable| ViewVariable {
                name: variable.name.clone(),
                secret: variable.secret,
                value: (!variable.secret).then(|| variable.value.clone()),
                has_value: !variable.value.is_empty(),
            })
            .collect(),
    }
}

fn valid_github_username(name: &str) -> bool {
    name.is_empty()
        || (name.len() <= 39
            && !name.starts_with('-')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
}
fn valid_text(value: &str, limit: usize) -> bool {
    !value.is_empty()
        && value.len() <= limit
        && !value.chars().any(|c| c.is_control())
}

/// The environment a real session gets. GitHub becomes `GH_TOKEN`/`GITHUB_TOKEN`
/// for `gh` and a per-host credential helper for `git`, so neither asks. The
/// helper is a shell one-liner that reads the token from the environment: no
/// secret reaches a file, an argument or a config the agent could commit.
fn policy(stored: &Stored) -> Result<LaunchPolicy, String> {
    let mut environment = Vec::new();
    for variable in &stored.variables {
        let value = if variable.secret {
            secret_store::unprotect(&variable.value).map_err(str::to_owned)?
        } else {
            variable.value.clone()
        };
        environment.push((variable.name.clone(), value));
    }
    if let Some(blob) = &stored.github_token {
        let token = secret_store::unprotect(blob).map_err(str::to_owned)?;
        let user = if stored.github_username.is_empty() {
            "x-access-token"
        } else {
            &stored.github_username
        };
        let helper = format!(
            "!f() {{ test \"$1\" = get && echo username={user} && echo password=$GH_TOKEN; }}; f"
        );
        let host = "credential.https://github.com.helper";
        environment.extend([
            ("GH_TOKEN".to_owned(), token.clone()),
            ("GITHUB_TOKEN".to_owned(), token),
            ("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned()),
            ("GIT_CONFIG_COUNT".to_owned(), "2".to_owned()),
            ("GIT_CONFIG_KEY_0".to_owned(), host.to_owned()),
            ("GIT_CONFIG_VALUE_0".to_owned(), String::new()),
            ("GIT_CONFIG_KEY_1".to_owned(), host.to_owned()),
            ("GIT_CONFIG_VALUE_1".to_owned(), helper),
        ]);
    }
    LaunchPolicy::new(stored.full_access, environment).map_err(|error| error.to_string())
}

/// Applies an edit to what is saved. A secret that is kept stays protected as it
/// was; turning a secret into a plain variable needs a new value, so a stored
/// secret is never revealed by flipping a checkbox.
fn merge(existing: &Stored, save: Save) -> Result<Stored, String> {
    if !valid_github_username(&save.github_username) {
        return Err("The GitHub username may contain only letters, digits and hyphens.".into());
    }
    let github_token = match save.github_token {
        SecretEdit::Keep => existing.github_token.clone(),
        SecretEdit::Clear => None,
        SecretEdit::Set(token) => {
            if !valid_text(&token, MAX_TOKEN_BYTES) || token.contains(char::is_whitespace) {
                return Err("The GitHub token is empty, too long or contains spaces.".into());
            }
            Some(secret_store::protect(&token).map_err(str::to_owned)?)
        }
    };
    if save.variables.len() > MAX_VARIABLES {
        return Err("Too many variables.".into());
    }
    let mut variables = Vec::with_capacity(save.variables.len());
    for item in save.variables {
        launch_policy::validate_name(&item.name).map_err(|error| error.to_string())?;
        let previous = existing
            .variables
            .iter()
            .find(|old| old.name.eq_ignore_ascii_case(&item.name));
        let value = match item.value {
            SecretEdit::Clear => {
                return Err(format!("{} has no value; remove the row instead.", item.name));
            }
            SecretEdit::Set(value) => {
                if !valid_text(&value, MAX_VALUE_BYTES) {
                    return Err(format!("{} needs a value without control characters.", item.name));
                }
                if item.secret {
                    secret_store::protect(&value).map_err(str::to_owned)?
                } else {
                    value
                }
            }
            SecretEdit::Keep => match previous {
                Some(old) if old.secret == item.secret => old.value.clone(),
                Some(old) if !old.secret => secret_store::protect(&old.value).map_err(str::to_owned)?,
                Some(_) => {
                    return Err(format!(
                        "{} was secret: type its value again to make it visible.",
                        item.name
                    ));
                }
                None => return Err(format!("{} needs a value.", item.name)),
            },
        };
        variables.push(StoredVariable {
            name: item.name,
            secret: item.secret,
            value,
        });
    }
    Ok(Stored {
        version: 1,
        full_access: save.full_access,
        github_username: save.github_username,
        github_token,
        variables,
    })
}

pub struct Controller {
    path: Result<PathBuf, String>,
    lock: Mutex<()>,
}
impl Controller {
    /// Reads the saved settings once and makes them the launch policy. A file that
    /// cannot be read leaves the protected default in force: failing closed is
    /// the safe side of a switch that removes protections.
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
    fn read(&self) -> Result<Stored, String> {
        let path = self.path()?;
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Stored::initial());
            }
            Err(_) => return Err("The launch settings could not be read.".into()),
        };
        if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
            return Err("The launch settings file is not usable; it was kept as it is.".into());
        }
        let text = fs::read_to_string(path)
            .map_err(|_| "The launch settings could not be read.".to_owned())?;
        let stored: Stored = serde_json::from_str(&text)
            .map_err(|_| "The launch settings file is damaged; it was kept as it is.".to_owned())?;
        if stored.version != 1 {
            return Err("The launch settings were saved by a newer Shinatama.".into());
        }
        Ok(stored)
    }
    fn write(&self, stored: &Stored) -> Result<(), String> {
        let path = self.path()?;
        let directory = path.parent().ok_or("The settings folder is unavailable.")?;
        fs::create_dir_all(directory)
            .map_err(|_| "The settings folder could not be created.".to_owned())?;
        let text = serde_json::to_vec_pretty(stored)
            .map_err(|_| "The launch settings could not be written.".to_owned())?;
        let temporary = path.with_extension("json.tmp");
        let result = (|| {
            let mut file = fs::File::create(&temporary)?;
            file.write_all(&text)?;
            file.sync_all()?;
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
            return Err("The launch settings could not be saved; the old ones were kept.".into());
        }
        Ok(())
    }
    /// Makes the saved settings the engine's policy; the protected default when
    /// they cannot be read or a secret cannot be opened.
    fn reload(&self) -> Result<Stored, String> {
        let result = self.read().and_then(|stored| policy(&stored).map(|p| (stored, p)));
        match result {
            Ok((stored, policy)) => {
                launch_policy::set_current(policy);
                Ok(stored)
            }
            Err(error) => {
                launch_policy::set_current(LaunchPolicy::default());
                Err(error)
            }
        }
    }
    fn load(&self) -> Result<View, String> {
        let _guard = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        self.reload().map(|stored| view(&stored))
    }
    fn save(&self, save: Save) -> Result<View, String> {
        let _guard = self.lock.lock().unwrap_or_else(|p| p.into_inner());
        // A damaged file does not block saving: the new settings replace it.
        let existing = self.read().unwrap_or_else(|_| Stored::initial());
        let next = merge(&existing, save)?;
        let next_policy = policy(&next)?;
        self.write(&next)?;
        launch_policy::set_current(next_policy);
        Ok(view(&next))
    }
}

#[tauri::command]
pub async fn launch_settings_load(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
) -> Result<View, String> {
    crate::local(&window)?;
    let controller = controller.inner().clone();
    tauri::async_runtime::spawn_blocking(move || controller.load())
        .await
        .map_err(|_| "The settings worker did not return.".to_owned())?
}
#[tauri::command]
pub async fn launch_settings_save(
    window: WebviewWindow,
    controller: State<'_, Arc<Controller>>,
    request: Save,
) -> Result<View, String> {
    crate::local(&window)?;
    let controller = controller.inner().clone();
    tauri::async_runtime::spawn_blocking(move || controller.save(request))
        .await
        .map_err(|_| "The settings worker did not return.".to_owned())?
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::MutexGuard;

    /// The launch policy is process-wide: tests that set it run one at a time.
    fn serial() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }
    fn directory(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("shinatama-launch-settings-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn controller(directory: &Path) -> Arc<Controller> {
        Arc::new(Controller {
            path: Ok(directory.join(FILE)),
            lock: Mutex::new(()),
        })
    }
    fn save(full_access: bool, token: SecretEdit, variables: Vec<SaveVariable>) -> Save {
        Save {
            full_access,
            github_username: "octocat".into(),
            github_token: token,
            variables,
        }
    }
    fn variable(name: &str, secret: bool, value: SecretEdit) -> SaveVariable {
        SaveVariable {
            name: name.into(),
            secret,
            value,
        }
    }
    fn environment(policy: &LaunchPolicy) -> std::collections::BTreeMap<OsString, OsString> {
        let mut environment = std::collections::BTreeMap::new();
        policy.apply_environment(&mut environment);
        environment
    }
    fn get<'a>(
        environment: &'a std::collections::BTreeMap<OsString, OsString>,
        name: &str,
    ) -> &'a OsString {
        environment.get(&OsString::from(name)).unwrap_or_else(|| panic!("{name}"))
    }

    #[test]
    fn a_fresh_install_runs_without_protections_and_adds_no_variables() {
        let _serial = serial();
        let dir = directory("fresh");
        let controller = controller(&dir);
        let view = controller.load().unwrap();
        assert!(view.full_access && !view.has_github_token && view.variables.is_empty());
        assert!(launch_policy::current().full_access());
        launch_policy::set_current(LaunchPolicy::default());
    }

    #[test]
    fn saving_protects_secrets_on_disk_and_never_returns_them() {
        let _serial = serial();
        let dir = directory("secrets");
        let controller = controller(&dir);
        let view = controller
            .save(save(
                true,
                SecretEdit::Set("ghp_realtoken123".into()),
                vec![
                    variable("API_KEY", true, SecretEdit::Set("s3cret-value".into())),
                    variable("REGION", false, SecretEdit::Set("eu-west-1".into())),
                ],
            ))
            .unwrap();
        assert!(view.has_github_token);
        let secret = &view.variables[0];
        assert!(secret.secret && secret.value.is_none() && secret.has_value);
        assert_eq!(view.variables[1].value.as_deref(), Some("eu-west-1"));
        let disk = fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(!disk.contains("ghp_realtoken123") && !disk.contains("s3cret-value"));
        assert!(disk.contains("eu-west-1"));
        let shown = serde_json::to_string(&view).unwrap();
        assert!(!shown.contains("ghp_realtoken123") && !shown.contains("s3cret-value"));
        launch_policy::set_current(LaunchPolicy::default());
    }

    #[test]
    fn github_becomes_token_variables_and_a_scoped_credential_helper() {
        let _serial = serial();
        let dir = directory("github");
        let controller = controller(&dir);
        controller
            .save(save(true, SecretEdit::Set("ghp_realtoken123".into()), vec![]))
            .unwrap();
        let environment = environment(&launch_policy::current());
        assert_eq!(get(&environment, "GH_TOKEN"), "ghp_realtoken123");
        assert_eq!(get(&environment, "GITHUB_TOKEN"), "ghp_realtoken123");
        assert_eq!(get(&environment, "GIT_TERMINAL_PROMPT"), "0");
        assert_eq!(get(&environment, "GIT_CONFIG_COUNT"), "2");
        assert_eq!(get(&environment, "GIT_CONFIG_KEY_0"), "credential.https://github.com.helper");
        assert_eq!(get(&environment, "GIT_CONFIG_VALUE_0"), "");
        let helper = get(&environment, "GIT_CONFIG_VALUE_1").to_string_lossy().into_owned();
        assert!(helper.contains("username=octocat") && helper.contains("password=$GH_TOKEN"));
        assert!(!helper.contains("ghp_realtoken123"), "the helper reads the token from the environment");
        launch_policy::set_current(LaunchPolicy::default());
    }

    #[test]
    fn keep_preserves_a_secret_and_clear_removes_the_token() {
        let _serial = serial();
        let dir = directory("keep");
        let controller = controller(&dir);
        controller
            .save(save(
                false,
                SecretEdit::Set("ghp_realtoken123".into()),
                vec![variable("API_KEY", true, SecretEdit::Set("s3cret-value".into()))],
            ))
            .unwrap();
        let view = controller
            .save(save(
                false,
                SecretEdit::Keep,
                vec![variable("API_KEY", true, SecretEdit::Keep)],
            ))
            .unwrap();
        assert!(view.has_github_token && view.variables[0].has_value);
        let environment = environment(&launch_policy::current());
        assert_eq!(get(&environment, "API_KEY"), "s3cret-value");
        assert!(!launch_policy::current().full_access());
        let view = controller
            .save(save(false, SecretEdit::Clear, vec![]))
            .unwrap();
        assert!(!view.has_github_token && view.variables.is_empty());
        assert!(!environment_has(&launch_policy::current(), "GH_TOKEN"));
        launch_policy::set_current(LaunchPolicy::default());
    }
    fn environment_has(policy: &LaunchPolicy, name: &str) -> bool {
        environment(policy).contains_key(&OsString::from(name))
    }

    #[test]
    fn a_secret_is_never_revealed_by_unticking_secret() {
        let _serial = serial();
        let dir = directory("reveal");
        let controller = controller(&dir);
        controller
            .save(save(
                true,
                SecretEdit::Keep,
                vec![variable("API_KEY", true, SecretEdit::Set("s3cret-value".into()))],
            ))
            .unwrap();
        let error = controller
            .save(save(true, SecretEdit::Keep, vec![variable("API_KEY", false, SecretEdit::Keep)]))
            .unwrap_err();
        assert!(error.contains("type its value again"));
        launch_policy::set_current(LaunchPolicy::default());
    }

    #[test]
    fn invalid_input_is_refused_and_leaves_the_saved_settings() {
        let _serial = serial();
        let dir = directory("invalid");
        let controller = controller(&dir);
        controller.save(save(true, SecretEdit::Keep, vec![])).unwrap();
        let before = fs::read(dir.join(FILE)).unwrap();
        for bad in [
            save(true, SecretEdit::Keep, vec![variable("PATH", false, SecretEdit::Set("x".into()))]),
            save(true, SecretEdit::Keep, vec![variable("1BAD", false, SecretEdit::Set("x".into()))]),
            save(true, SecretEdit::Keep, vec![variable("A", false, SecretEdit::Set("".into()))]),
            save(true, SecretEdit::Keep, vec![variable("A", false, SecretEdit::Set("a\nb".into()))]),
            save(true, SecretEdit::Keep, vec![variable("A", false, SecretEdit::Keep)]),
            save(true, SecretEdit::Set("has space".into()), vec![]),
            save(true, SecretEdit::Set("".into()), vec![]),
            Save {
                github_username: "-bad name".into(),
                ..save(true, SecretEdit::Keep, vec![])
            },
        ] {
            assert!(controller.save(bad).is_err());
        }
        assert_eq!(fs::read(dir.join(FILE)).unwrap(), before);
        launch_policy::set_current(LaunchPolicy::default());
    }

    #[test]
    fn a_damaged_file_fails_closed_and_a_save_replaces_it() {
        let _serial = serial();
        let dir = directory("damaged");
        fs::write(dir.join(FILE), b"{ not json").unwrap();
        launch_policy::set_current(LaunchPolicy::new(true, vec![]).unwrap());
        let controller = controller(&dir);
        assert!(controller.load().unwrap_err().contains("damaged"));
        assert!(!launch_policy::current().full_access(), "unreadable settings must not leave full access on");
        assert_eq!(fs::read(dir.join(FILE)).unwrap(), b"{ not json", "the damaged file is kept until a save");
        controller.save(save(true, SecretEdit::Keep, vec![])).unwrap();
        assert!(controller.load().unwrap().full_access);
        launch_policy::set_current(LaunchPolicy::default());
    }
}
