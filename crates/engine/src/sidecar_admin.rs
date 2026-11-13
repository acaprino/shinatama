//! Read-only client for the local sidecar admin endpoints (`/__bili/*`).
//!
//! Every read first proves, on the same connection, that the listener is the
//! process Shinatama started (health `ok` and `pid`), and only then asks for
//! the requested path. Nothing here ever writes.

use crate::sidecar_supervisor::{connect, request, request_limited};
use serde::Serialize;
use serde_json::Value;
use std::{
    num::NonZeroU16,
    time::{Duration, Instant},
};

/// Largest admin body accepted by the typed readers below.
const ADMIN_BODY_LIMIT: usize = 256 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct AdminTarget {
    pub port: NonZeroU16,
    pub pid: u32,
    pub timeout: Duration,
}

/// GET /__bili/health then `path` on ONE connection; None unless health is 200,
/// ok==true and pid matches. Body capped at `limit` bytes.
pub fn get_json(target: &AdminTarget, path: &str, limit: usize) -> Option<Value> {
    // Only admin paths, and nothing that could split the request line.
    if !path.starts_with("/__bili/") || path.bytes().any(|byte| byte <= b' ' || byte == 0x7f) {
        return None;
    }
    let until = Instant::now() + target.timeout;
    let mut stream = connect(target.port, target.timeout)?;
    let (200, health) = request(
        &mut stream,
        target.port,
        "GET",
        "/__bili/health",
        true,
        until,
    )?
    else {
        return None;
    };
    let health = serde_json::from_slice::<Value>(&health).ok()?;
    if health["ok"] != true || health["pid"].as_u64() != Some(target.pid.into()) {
        return None;
    }
    let (200, body) = request_limited(&mut stream, target.port, "GET", path, false, until, limit)?
    else {
        return None;
    };
    serde_json::from_slice(&body).ok()
}

/// `inFlight` from /__bili/status. None = unknown, never zero.
pub fn in_flight(target: &AdminTarget) -> Option<u64> {
    get_json(target, "/__bili/status", ADMIN_BODY_LIMIT)?["inFlight"].as_u64()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedBaseline {
    pub prompt_rewrite_mode: Option<String>,
    pub tiers: Option<bool>,
    pub absorb_enabled: Option<bool>,
    pub absorb_min_tool_tokens: Option<u64>,
    pub ccr_enabled: Option<bool>,
    pub ccr_min_tool_tokens: Option<u64>,
    pub adaptive_enabled: Option<bool>,
    pub adaptive_max_tokens: Option<u64>,
    pub steering_enabled: Option<bool>,
    pub verbosity_level: Option<u64>,
    pub effort_routing: Option<bool>,
}

/// Parsed from the `compress` and `promptRewrite` objects of GET /__bili/config.
pub fn resolved_baseline(target: &AdminTarget) -> Option<ResolvedBaseline> {
    get_json(target, "/__bili/config", ADMIN_BODY_LIMIT)
        .map(|value| baseline_from_config_value(&value))
}

/// The configuration may be served bare or inside a `config` envelope; a value
/// that is absent or of the wrong type stays unknown rather than defaulting.
pub fn baseline_from_config_value(config: &Value) -> ResolvedBaseline {
    let root = if config["compress"].is_object() {
        config
    } else {
        &config["config"]
    };
    let compress = &root["compress"];
    let absorb = &compress["absorb"];
    let ccr = &compress["ccr"];
    let adaptive = &compress["adaptivePressure"];
    let steering = &compress["outputSteering"];
    ResolvedBaseline {
        prompt_rewrite_mode: prompt_rewrite_mode(config, root),
        tiers: compress["tiers"].as_bool(),
        absorb_enabled: absorb["enabled"].as_bool(),
        absorb_min_tool_tokens: absorb["minToolTokens"].as_u64(),
        ccr_enabled: ccr["enabled"].as_bool(),
        ccr_min_tool_tokens: ccr["minToolTokens"].as_u64(),
        adaptive_enabled: adaptive["enabled"].as_bool(),
        adaptive_max_tokens: adaptive["maxTokens"].as_u64(),
        steering_enabled: steering["enabled"].as_bool(),
        verbosity_level: steering["verbosityLevel"].as_u64(),
        effort_routing: steering["effortRouting"].as_bool(),
    }
}

/// `promptRewrite` is not a top-level key of the served configuration: the mode
/// lives in `raw`, the JSON text of the configuration file. A direct value wins;
/// a `raw` that is absent, not a string or not valid JSON leaves the mode unknown.
fn prompt_rewrite_mode(config: &Value, root: &Value) -> Option<String> {
    if let Some(mode) = root["promptRewrite"]["mode"].as_str() {
        return Some(mode.to_owned());
    }
    let raw = root["raw"].as_str().or_else(|| config["raw"].as_str())?;
    let file = serde_json::from_str::<Value>(raw).ok()?;
    file["promptRewrite"]["mode"].as_str().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar_supervisor::test_support::server_with;

    fn target(port: NonZeroU16, pid: u32) -> AdminTarget {
        AdminTarget {
            port,
            pid,
            timeout: Duration::from_millis(500),
        }
    }

    #[test]
    fn in_flight_is_read_only_after_identity_matches() {
        let (port, worker) = server_with(4242, vec![r#"{"inFlight":3}"#.into()]);
        assert_eq!(in_flight(&target(port, 4242)), Some(3));
        // One connection, GET only, identity first.
        assert_eq!(
            worker.join().unwrap(),
            ["GET /__bili/health HTTP/1.1", "GET /__bili/status HTTP/1.1"]
        );
        // Another identity: the status path is never requested.
        let (port, worker) = server_with(4242, vec![r#"{"inFlight":3}"#.into()]);
        assert_eq!(in_flight(&target(port, 9999)), None);
        assert_eq!(worker.join().unwrap(), ["GET /__bili/health HTTP/1.1"]);
    }

    #[test]
    fn missing_or_non_numeric_in_flight_is_unknown() {
        for body in [
            r#"{"inFlight":"3"}"#,
            "{}",
            r#"{"inFlight":-1}"#,
            r#"{"inFlight":1.5}"#,
            "[]",
            "not json",
        ] {
            let (port, worker) = server_with(4242, vec![body.into()]);
            assert_eq!(in_flight(&target(port, 4242)), None, "{body}");
            worker.join().unwrap();
        }
    }

    #[test]
    fn oversized_body_is_refused() {
        let big = format!(r#"{{"pad":"{}"}}"#, "x".repeat(300 * 1024));
        let (port, worker) = server_with(4242, vec![big]);
        assert_eq!(
            get_json(&target(port, 4242), "/__bili/config", 256 * 1024),
            None
        );
        worker.join().unwrap();
        // The same body is accepted when the caller's limit allows it.
        let big = format!(r#"{{"pad":"{}"}}"#, "x".repeat(300 * 1024));
        let (port, worker) = server_with(4242, vec![big]);
        assert!(get_json(&target(port, 4242), "/__bili/config", 400 * 1024).is_some());
        worker.join().unwrap();
    }

    #[test]
    fn only_admin_paths_are_requested() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = NonZeroU16::new(listener.local_addr().unwrap().port()).unwrap();
        for path in ["/other", "/__bili/x y", "/__bili/x\r\nHost: y", ""] {
            assert_eq!(get_json(&target(port, 4242), path, 1024), None, "{path:?}");
        }
        // A refused path never even opens a connection.
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn baseline_parser_reads_the_written_baseline_exactly() {
        let value: serde_json::Value =
            serde_json::from_slice(&crate::sidecar_config::baseline_bytes_for_test()).unwrap();
        let parsed = baseline_from_config_value(&value);
        assert_eq!(parsed.ccr_min_tool_tokens, Some(4000));
        assert_eq!(parsed.adaptive_max_tokens, Some(160000));
        assert_eq!(parsed.verbosity_level, Some(2));
        assert_eq!(parsed.effort_routing, Some(false));
        assert_eq!(parsed.prompt_rewrite_mode.as_deref(), Some("auto"));
        assert_eq!(parsed.tiers, Some(true));
        assert_eq!(parsed.absorb_enabled, Some(true));
        assert_eq!(parsed.absorb_min_tool_tokens, Some(4000));
        assert_eq!(parsed.ccr_enabled, Some(true));
        assert_eq!(parsed.adaptive_enabled, Some(true));
        assert_eq!(parsed.steering_enabled, Some(true));
        // The same configuration inside an envelope parses identically.
        let wrapped = serde_json::json!({ "config": value });
        assert_eq!(baseline_from_config_value(&wrapped), parsed);
    }

    /// Shaped like the real `GET /__bili/config`: `compress` at the top, no
    /// top-level `promptRewrite`, the file text as a JSON string in `raw`.
    fn served_shape(raw: serde_json::Value) -> serde_json::Value {
        let file: serde_json::Value =
            serde_json::from_slice(&crate::sidecar_config::baseline_bytes_for_test()).unwrap();
        serde_json::json!({
            "path": "C:\\synthetic-sidecar\\config.json",
            "providers": {},
            "upstreamProxy": null,
            "compress": file["compress"],
            "raw": raw,
            "passthrough": false
        })
    }

    #[test]
    fn baseline_parser_reads_the_mode_from_the_served_raw_text() {
        let raw = String::from_utf8(crate::sidecar_config::baseline_bytes_for_test()).unwrap();
        let served = served_shape(serde_json::Value::String(raw));
        assert!(served.get("promptRewrite").is_none());
        assert_eq!(
            baseline_from_config_value(&served),
            ResolvedBaseline {
                prompt_rewrite_mode: Some("auto".into()),
                tiers: Some(true),
                absorb_enabled: Some(true),
                absorb_min_tool_tokens: Some(4000),
                ccr_enabled: Some(true),
                ccr_min_tool_tokens: Some(4000),
                adaptive_enabled: Some(true),
                adaptive_max_tokens: Some(160000),
                steering_enabled: Some(true),
                verbosity_level: Some(2),
                effort_routing: Some(false),
            }
        );
        // A direct value is preferred over the raw text.
        let mut direct = served.clone();
        direct["promptRewrite"] = serde_json::json!({ "mode": "off" });
        assert_eq!(
            baseline_from_config_value(&direct)
                .prompt_rewrite_mode
                .as_deref(),
            Some("off")
        );
    }

    #[test]
    fn unreadable_raw_leaves_the_mode_unknown_but_compress_is_still_read() {
        for raw in [
            serde_json::json!("{ not json"),
            serde_json::json!(42),
            serde_json::json!(null),
            serde_json::json!(r#"{"promptRewrite":{"mode":7}}"#),
            serde_json::json!(r#"{"other":true}"#),
        ] {
            let parsed = baseline_from_config_value(&served_shape(raw.clone()));
            assert_eq!(parsed.prompt_rewrite_mode, None, "{raw}");
            assert_eq!(parsed.ccr_min_tool_tokens, Some(4000), "{raw}");
            assert_eq!(parsed.adaptive_max_tokens, Some(160000), "{raw}");
            assert_eq!(parsed.verbosity_level, Some(2), "{raw}");
            assert_eq!(parsed.effort_routing, Some(false), "{raw}");
        }
    }

    #[test]
    fn baseline_parser_leaves_unreadable_values_unknown() {
        let parsed = baseline_from_config_value(&serde_json::json!({
            "promptRewrite": { "mode": 3 },
            "compress": { "ccr": { "minToolTokens": "4000" }, "outputSteering": { "verbosityLevel": -1 } }
        }));
        assert_eq!(parsed.prompt_rewrite_mode, None);
        assert_eq!(parsed.ccr_min_tool_tokens, None);
        assert_eq!(parsed.verbosity_level, None);
        assert_eq!(parsed.tiers, None);
        let empty = baseline_from_config_value(&serde_json::json!({}));
        assert_eq!(empty.adaptive_max_tokens, None);
        assert_eq!(empty.effort_routing, None);
    }

    #[test]
    fn resolved_baseline_reads_the_served_configuration() {
        let body = serde_json::json!({ "config": serde_json::from_slice::<serde_json::Value>(
            &crate::sidecar_config::baseline_bytes_for_test()).unwrap() })
        .to_string();
        let (port, worker) = server_with(4242, vec![body]);
        let parsed = resolved_baseline(&target(port, 4242)).unwrap();
        assert_eq!(parsed.ccr_min_tool_tokens, Some(4000));
        assert_eq!(
            worker.join().unwrap(),
            ["GET /__bili/health HTTP/1.1", "GET /__bili/config HTTP/1.1"]
        );
        let (port, worker) = server_with(4242, vec!["{}".into()]);
        assert_eq!(resolved_baseline(&target(port, 9999)), None);
        worker.join().unwrap();
    }
}
