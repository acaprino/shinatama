#![cfg(windows)]
//! Separate ignored live-traffic effective-config proof (C12.4 percorso B):
//! the actual Claude CLI talks through the canonical sidecar to a synthetic
//! loopback origin, which observes the sidecar's effective configuration on
//! the real upstream request bodies. No product Start authority, no account,
//! no paid provider call.
#[path = "support/claude_native.rs"]
mod claude_native;
use claude_native::{Fixture, Result, Scenario, ec_arguments, require, write_evidence};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};
const MARKER: &str = "SHINATAMA_SYNTHETIC_CLAUDE_TLS_OK";
#[test]
#[ignore = "root must review the fixed local Claude effective-config recipe before explicitly scheduling this native proof"]
fn canonical_sidecar_applies_effective_config_to_live_claude_traffic() {
    let parent_tls = [
        "NODE_EXTRA_CA_CERTS",
        "SSL_CERT_FILE",
        "NODE_TLS_REJECT_UNAUTHORIZED",
        "BUN_TLS_REJECT_UNAUTHORIZED",
    ]
    .map(std::env::var_os);
    let result = qualify();
    assert_eq!(
        parent_tls,
        [
            "NODE_EXTRA_CA_CERTS",
            "SSL_CERT_FILE",
            "NODE_TLS_REJECT_UNAUTHORIZED",
            "BUN_TLS_REJECT_UNAUTHORIZED"
        ]
        .map(std::env::var_os)
    );
    assert!(
        result.is_ok(),
        "{}",
        result.err().unwrap_or("unknown-local-proof")
    );
}
fn qualify() -> Result<()> {
    let mut fixture = Fixture::prepare(Scenario::EffectiveConfig)?;
    let observed = observe(&mut fixture);
    // Release every exact owner even when a case fails, before propagating it.
    fixture.release_all()?;
    let mut evidence = fixture.evidence(observed?)?;
    evidence["parentTlsEnvironmentUnchanged"] = true.into();
    evidence["scope"] = json!(
        ("separate C09 native headless fixture; live upstream bodies observed at the synthetic loopback origin; in-process runtime introspection (percorso A endpoint) not performed")
    );
    write_evidence("claude-effective-config.json", &evidence)
}
fn observe(fixture: &mut Fixture) -> Result<Value> {
    fixture.start_sidecar()?;
    let ready = fixture.phase("case-ready")?;
    require(
        ready["operation"].as_str() == Some("main"),
        "ordered-effective-config-case",
    )?;
    let case = fixture.create_case("effective-config")?;
    let command = fixture.claude_command(&case, true, ec_arguments(), BTreeMap::new())?;
    let index = fixture.start_claude(command)?;
    let exit = fixture.wait_exit(index, Duration::from_secs(20))?;
    let (output, errors) = fixture.output(index)?;
    let output = String::from_utf8_lossy(output);
    let errors = String::from_utf8_lossy(errors);
    let marker_observed = output.contains(MARKER);
    require(exit == 0, "trusted-claude-exit")?;
    require(marker_observed, "trusted-claude-marker")?;
    let _ = errors;
    fixture.release(index)?;
    fixture.send(
        json!({"operation":"main","exitCode":exit,"markerObserved":marker_observed,
        "ownerReleased":true,"outputBounded":true}),
    )?;
    let record = fixture.phase("case-finished")?;
    require(
        record["steeringSentinelObserved"].as_bool() == Some(true),
        "steering-effect-missing-on-live-upstream-body",
    )?;
    require(
        record["messageRequests"]
            .as_u64()
            .is_some_and(|count| count > 0),
        "live-upstream-traffic-required",
    )?;
    fixture.shutdown()
}
