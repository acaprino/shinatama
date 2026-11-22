//! `mukade shinatama-kit <opencode|pi> <folder> <endpoint> <service-id>`: Mukade's own launch kit
//! for a harness whose chat adapter is a plugin (ADR-0057), written into `folder`, for Shinatama's
//! launcher to compose into the native launch it already owns. It calls Mukade's
//! `Harness::chat_launch` exactly as `mukade chat launch` does, and starts nothing: of the command
//! that function returns, only the arguments it adds and, for OpenCode, the merged inline
//! configuration are printed, as one JSON object on standard output.
//!
//! OpenCode's merge starts from the configuration Shinatama would give the session, passed in this
//! process's `OPENCODE_CONFIG_CONTENT`, as Mukade's launcher starts from the inherited one. The
//! service's credential is never an input or an output: Shinatama puts it in the child's
//! environment itself (`MUKADE_CHAT_AUTHORIZATION`, which the plugins read).
use mukade::chat::adapter::{Kit, STOP_HOOK_TIMEOUT};
use serde_json::{Value, json};
use std::path::PathBuf;

pub const COMMAND: &str = "shinatama-kit";

pub fn run(args: &[String]) -> i32 {
    match write(args) {
        Ok(value) => {
            println!("{value}");
            0
        }
        Err(message) => {
            eprintln!("invalid_arguments: {message}");
            2
        }
    }
}

fn write(args: &[String]) -> Result<Value, String> {
    let [harness, folder, endpoint, service_id] = args else {
        return Err(format!(
            "usage: mukade {COMMAND} <opencode|pi> <folder> <endpoint> <service-id>"
        ));
    };
    let id: mukade::HarnessId = match harness.as_str() {
        "opencode" | "pi" => harness.parse().map_err(|_| "unknown harness".to_owned())?,
        _ => return Err("only opencode and pi have a plugin kit".into()),
    };
    let folder = PathBuf::from(folder);
    if !folder.is_absolute() {
        return Err("the kit folder must be absolute".into());
    }
    let kit = Kit {
        exe: std::env::current_exe()
            .map_err(|_| "the mukade executable cannot be found".to_owned())?,
        folder,
        server: mukade::chat::connect::Server {
            endpoint: endpoint.clone(),
            token: String::new(),
            service_id: service_id.clone(),
        },
        stop_timeout: STOP_HOOK_TIMEOUT,
        args: Vec::new(),
    };
    let command = mukade::harnesses::get(id)
        .chat_launch(&kit)
        .map_err(|error| format!("{}: {}", error.code, error.message))?
        .ok_or("this harness has no chat adapter")?;
    Ok(projection(id, &command))
}

/// What Shinatama composes: the arguments the kit adds (OpenCode's `--standalone` is left to
/// Shinatama's launcher, which always passes it) and OpenCode's merged configuration. The
/// credential variables Mukade's launcher sets are left out.
fn projection(id: mukade::HarnessId, command: &std::process::Command) -> Value {
    let arguments: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .filter(|arg| arg != "--standalone")
        .collect();
    let config = command
        .get_envs()
        .find(|(name, _)| *name == "OPENCODE_CONFIG_CONTENT")
        .and_then(|(_, value)| value.map(|value| value.to_string_lossy().into_owned()));
    json!({"harness": id.as_str(), "arguments": arguments, "opencode_config": config})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("shinatama-kit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn pi_s_kit_is_one_extension_argument_and_holds_no_credential() {
        let folder = temp("pi");
        let value = write(&[
            "pi".into(),
            folder.to_string_lossy().into(),
            "http://127.0.0.1:1/mcp".into(),
            "svc".into(),
        ])
        .unwrap();
        let arguments = value["arguments"].as_array().unwrap();
        assert_eq!(arguments.len(), 2);
        assert_eq!(arguments[0], "-e");
        let extension = PathBuf::from(arguments[1].as_str().unwrap());
        let source = std::fs::read_to_string(&extension).unwrap();
        assert!(
            extension.starts_with(&folder)
                && source.contains("registerMcpServer")
                && source.contains("http://127.0.0.1:1/mcp")
        );
        assert!(!value.to_string().contains("Bearer"));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn opencode_s_kit_merges_into_the_configuration_it_is_given() {
        let folder = temp("opencode");
        // SAFETY: tests of this crate that touch this variable run in this one test only.
        unsafe {
            std::env::set_var(
                "OPENCODE_CONFIG_CONTENT",
                r#"{"update":"disable","share":"disabled"}"#,
            )
        };
        let value = write(&[
            "opencode".into(),
            folder.to_string_lossy().into(),
            "http://127.0.0.1:1/mcp".into(),
            "svc".into(),
        ])
        .unwrap();
        let config: Value =
            serde_json::from_str(value["opencode_config"].as_str().unwrap()).unwrap();
        assert_eq!(config["update"], "disable");
        assert_eq!(config["share"], "disabled");
        let plugin = PathBuf::from(config["plugins"][0].as_str().unwrap());
        assert!(plugin.starts_with(&folder) && plugin.join("server.js").is_file());
        assert_eq!(
            config["mcp"]["servers"]["mukade"]["url"],
            "http://127.0.0.1:1/mcp"
        );
        assert_eq!(
            config["mcp"]["servers"]["mukade"]["headers"]["Authorization"],
            "{env:MUKADE_CHAT_AUTHORIZATION}"
        );
        assert_eq!(value["arguments"], json!([]));
        assert!(
            write(&[
                "codex".into(),
                folder.to_string_lossy().into(),
                "e".into(),
                "s".into()
            ])
            .is_err()
        );
        assert!(write(&["pi".into(), "relative".into(), "e".into(), "s".into()]).is_err());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
