//! Shared connection definitions and per-launch translations. No plaintext key is serializable.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fmt,
    sync::{OnceLock, RwLock},
};

pub const KEY_ENV: &str = "SHINATAMA_MODEL_API_KEY";
pub const MAX_CONNECTIONS: usize = 16;
pub const MAX_MODELS: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Api {
    OpenaiCompletions,
    OpenaiResponses,
    AnthropicMessages,
    GoogleGenerativeAi,
}
impl Api {
    fn npm(self) -> &'static str {
        match self {
            Self::OpenaiCompletions => "@ai-sdk/openai-compatible",
            Self::OpenaiResponses => "@ai-sdk/openai",
            Self::AnthropicMessages => "@ai-sdk/anthropic",
            Self::GoogleGenerativeAi => "@ai-sdk/google",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    pub context_window: u32,
    pub max_tokens: u32,
    pub reasoning: bool,
    pub vision: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub api: Api,
    pub base_url: String,
    pub models: Vec<Model>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Selection {
    pub connection_id: String,
    pub model_id: String,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Assignments {
    pub opencode: Option<Selection>,
    pub pi: Option<Selection>,
    pub hermes: Option<Selection>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Configuration {
    pub connections: Vec<Connection>,
    pub assignments: Assignments,
}

fn token(text: &str, max: usize, model: bool) -> bool {
    !text.is_empty()
        && text.len() <= max
        && !text.starts_with('-')
        && text.bytes().all(|c| {
            c.is_ascii_alphanumeric() || b"._-".contains(&c) || (model && b"/:".contains(&c))
        })
}
impl Configuration {
    pub fn validate(&self) -> Result<(), String> {
        if self.connections.len() > MAX_CONNECTIONS {
            return Err("At most 16 connections are allowed.".into());
        }
        let mut ids = BTreeSet::new();
        for connection in &self.connections {
            if !token(&connection.id, 64, false) || !ids.insert(&connection.id) {
                return Err("Connection ids must be valid and unique.".into());
            }
            if connection.name.trim().is_empty()
                || connection.name.len() > 100
                || connection
                    .name
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '{' | '}'))
            {
                return Err("Each connection needs a name of at most 100 bytes without braces or control characters.".into());
            }
            let url = &connection.base_url;
            if url.len() > 2048
                || !(url.starts_with("https://") || url.starts_with("http://"))
                || url
                    .chars()
                    .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '{' | '}'))
            {
                return Err(
                    "The base URL must be an HTTP or HTTPS address without placeholders.".into(),
                );
            }
            if connection.models.is_empty() || connection.models.len() > MAX_MODELS {
                return Err("Each connection needs between 1 and 64 models.".into());
            }
            let mut models = BTreeSet::new();
            for model in &connection.models {
                if !token(&model.id, 200, true) || !models.insert(&model.id) {
                    return Err("Model ids must be unique within a connection and use letters, digits, dots, underscores, hyphens, slashes or colons.".into());
                }
                if model.context_window == 0
                    || model.context_window > 10_000_000
                    || model.max_tokens == 0
                    || model.max_tokens > model.context_window
                {
                    return Err("Model limits must be positive; output cannot exceed context (maximum 10,000,000 tokens).".into());
                }
            }
        }
        for selection in [
            &self.assignments.opencode,
            &self.assignments.pi,
            &self.assignments.hermes,
        ]
        .into_iter()
        .flatten()
        {
            if !self.connections.iter().any(|c| {
                c.id == selection.connection_id
                    && c.models.iter().any(|m| m.id == selection.model_id)
            }) {
                return Err(
                    "A selected model no longer exists. Choose another model or native settings."
                        .into(),
                );
            }
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub configuration: Configuration,
    pub keys: BTreeMap<String, String>,
}
impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModelSnapshot")
            .field("connections", &self.configuration.connections.len())
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Copy)]
pub enum Harness {
    OpenCode,
    Pi,
}
impl Snapshot {
    pub fn selected(&self, harness: Harness) -> Option<(&Connection, &Selection)> {
        let selection = match harness {
            Harness::OpenCode => &self.configuration.assignments.opencode,
            Harness::Pi => &self.configuration.assignments.pi,
        }
        .as_ref()?;
        self.configuration
            .connections
            .iter()
            .find(|c| c.id == selection.connection_id)
            .map(|c| (c, selection))
    }
    /// Never inherit another session's provider key, even when native settings are selected.
    pub fn apply_key(&self, harness: Harness, environment: &mut BTreeMap<OsString, OsString>) {
        environment.retain(|name, _| !name.eq_ignore_ascii_case(KEY_ENV));
        if let Some((connection, _)) = self.selected(harness) {
            environment.insert(
                KEY_ENV.into(),
                self.keys
                    .get(&connection.id)
                    .map(String::as_str)
                    .unwrap_or("not-required")
                    .into(),
            );
        }
    }
    pub fn opencode(&self, mut base: Value) -> Value {
        if let Some((connection, selection)) = self.selected(Harness::OpenCode) {
            let alias = alias(connection);
            let models: BTreeMap<_, _> = connection.models.iter().map(|m| (m.id.clone(), json!({
                "name": m.id, "limit": {"context": m.context_window, "output": m.max_tokens},
                "reasoning": m.reasoning, "modalities": {"input": if m.vision { vec!["text", "image"] } else { vec!["text"] }, "output": ["text"]}
            }))).collect();
            base["provider"][&alias] = json!({"name": connection.name, "npm": connection.api.npm(),
                "options": {"baseURL": endpoint(connection, Harness::OpenCode), "apiKey": format!("{{env:{KEY_ENV}}}")}, "models": models});
            base["model"] = json!(format!("{alias}/{}", selection.model_id));
        }
        base
    }
    /// The extension is generated from JSON literals only; credentials stay in the process environment.
    pub fn pi_extension(&self) -> Option<String> {
        let (connection, _) = self.selected(Harness::Pi)?;
        let models: Vec<_> = connection
            .models
            .iter()
            .map(|m| {
                json!({"id": m.id, "name": m.id,
            "contextWindow": m.context_window, "maxTokens": m.max_tokens, "reasoning": m.reasoning,
            "input": if m.vision { vec!["text", "image"] } else { vec!["text"] },
            "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}})
            })
            .collect();
        let config = json!({"name": connection.name, "api": connection.api, "baseUrl": endpoint(connection, Harness::Pi), "models": models});
        // Older Pi resolves an entire config string as an environment-variable name;
        // newer Pi interpolates $NAME. The alias supports both without version gating.
        // Neither resolver recursively interprets the secret returned from the environment.
        Some(format!(
            r#"export default function(pi) {{
  const config = {config};
  config.apiKey = "${KEY_ENV}";
  process.env[config.apiKey] = process.env.{KEY_ENV};
  const providerId = {};
  pi.registerProvider(providerId, config);
  pi.on("session_start", (_event, ctx) => {{
    const registry = ctx.modelRegistry;
    if (typeof registry.getProvider !== "function" || typeof registry.getRegisteredNativeProvider !== "function") {{
      if (typeof registry.authStorage?.setRuntimeApiKey === "function") {{
        registry.authStorage.setRuntimeApiKey(providerId, process.env.{KEY_ENV});
        return;
      }}
      ctx.ui.notify("This Pi build gives native /login credentials priority. Use /logout for this connection if its shared key is ignored.", "warning");
      return;
    }}
    const provider = registry.getProvider(providerId);
    if (!provider) return;
    pi.registerProvider({{
      ...provider,
      auth: {{ apiKey: {{
        name: "Managed by Shinatama",
        check: async () => ({{ type: "api_key", source: "Shinatama" }}),
        resolve: async () => ({{ auth: {{ apiKey: process.env.{KEY_ENV} }}, source: "Shinatama" }})
      }} }}
    }});
  }});
}}
"#,
            json!(alias(connection))
        ))
    }
}
fn endpoint(connection: &Connection, harness: Harness) -> String {
    let base = connection.base_url.trim_end_matches('/');
    if connection.api == Api::AnthropicMessages {
        let root = base.strip_suffix("/v1").unwrap_or(base);
        match harness {
            Harness::OpenCode => format!("{root}/v1"),
            Harness::Pi => root.into(),
        }
    } else {
        base.into()
    }
}
pub fn alias(connection: &Connection) -> String {
    format!("shinatama-{}", connection.id)
}
fn cell() -> &'static RwLock<Result<Snapshot, String>> {
    static CELL: OnceLock<RwLock<Result<Snapshot, String>>> = OnceLock::new();
    CELL.get_or_init(|| RwLock::new(Ok(Snapshot::default())))
}
pub fn set_current(snapshot: Result<Snapshot, String>) {
    *cell().write().unwrap_or_else(|p| p.into_inner()) = snapshot;
}
pub fn current() -> Result<Snapshot, String> {
    cell().read().unwrap_or_else(|p| p.into_inner()).clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        let c = Connection {
            id: "example".into(),
            name: "Local \"model\"".into(),
            api: Api::OpenaiCompletions,
            base_url: "http://localhost:1234/v1".into(),
            models: vec![Model {
                id: "org/model:v2".into(),
                context_window: 32000,
                max_tokens: 4000,
                reasoning: true,
                vision: false,
            }],
        };
        let selected = Some(Selection {
            connection_id: c.id.clone(),
            model_id: c.models[0].id.clone(),
        });
        Snapshot {
            configuration: Configuration {
                connections: vec![c],
                assignments: Assignments {
                    opencode: selected.clone(),
                    pi: selected,
                    hermes: None,
                },
            },
            keys: BTreeMap::from([("example".into(), "synthetic-secret".into())]),
        }
    }
    #[test]
    fn shared_selection_translates_without_persisting_a_key_or_replacing_permissions() {
        let s = snapshot();
        s.configuration.validate().unwrap();
        let config = s.opencode(json!({"permission": "allow", "plugin": []}));
        assert_eq!(config["permission"], "allow");
        assert_eq!(config["model"], "shinatama-example/org/model:v2");
        assert_eq!(
            config["provider"]["shinatama-example"]["options"]["apiKey"],
            "{env:SHINATAMA_MODEL_API_KEY}"
        );
        assert!(
            s.pi_extension()
                .unwrap()
                .contains("process.env.SHINATAMA_MODEL_API_KEY")
        );
        for text in [
            config.to_string(),
            s.pi_extension().unwrap(),
            format!("{s:?}"),
        ] {
            assert!(!text.contains("synthetic-secret"));
        }
        let mut env = BTreeMap::new();
        s.apply_key(Harness::Pi, &mut env);
        assert_eq!(env[&OsString::from(KEY_ENV)], "synthetic-secret");
    }
    #[test]
    fn anthropic_urls_match_each_sdk_without_duplicate_version_paths() {
        for url in [
            "https://proxy.example/anthropic",
            "https://proxy.example/anthropic/",
            "https://proxy.example/anthropic/v1",
        ] {
            let mut s = snapshot();
            s.configuration.connections[0].api = Api::AnthropicMessages;
            s.configuration.connections[0].base_url = url.into();
            assert_eq!(
                s.opencode(json!({}))["provider"]["shinatama-example"]["options"]["baseURL"],
                "https://proxy.example/anthropic/v1"
            );
            assert!(
                s.pi_extension()
                    .unwrap()
                    .contains("\"baseUrl\":\"https://proxy.example/anthropic\"")
            );
        }
    }
    #[test]
    fn pi_keys_are_references_never_executable_configuration_values() {
        let s = snapshot();
        let source = s.pi_extension().unwrap();
        assert!(source.contains("config.apiKey = \"$SHINATAMA_MODEL_API_KEY\""));
        assert!(
            source.contains("process.env[config.apiKey] = process.env.SHINATAMA_MODEL_API_KEY")
        );
        assert!(!source.contains("config.apiKey = process.env"));
    }
    #[test]
    fn native_settings_leave_configuration_untouched_and_remove_inherited_managed_keys() {
        let s = Snapshot::default();
        let base = json!({"permission": "ask"});
        assert_eq!(s.opencode(base.clone()), base);
        assert!(s.pi_extension().is_none());
        let mut env = BTreeMap::from([(KEY_ENV.to_ascii_lowercase().into(), "old-secret".into())]);
        s.apply_key(Harness::Pi, &mut env);
        assert!(env.is_empty());
    }
    #[test]
    fn rejects_dangling_selections_duplicates_interpolation_and_invalid_limits() {
        let original = snapshot().configuration;
        let mut c = original.clone();
        c.connections.clear();
        assert!(c.validate().is_err());
        let mut c = original.clone();
        c.connections.push(c.connections[0].clone());
        assert!(c.validate().is_err());
        for id in ["-flag", "{env:SECRET}", "a\n", "x y", "!command"] {
            let mut c = original.clone();
            c.connections[0].models[0].id = id.into();
            assert!(c.validate().is_err());
        }
        let mut c = original.clone();
        c.connections[0].models[0].max_tokens = 32001;
        assert!(c.validate().is_err());
        let mut c = original;
        c.connections[0].base_url = "https://{env:SECRET}".into();
        assert!(c.validate().is_err());
    }
}
