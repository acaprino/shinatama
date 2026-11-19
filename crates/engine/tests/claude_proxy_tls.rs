#![cfg(windows)]
//! Separate ignored actual Claude/Bun TLS proof; no product Start authority.
#[path = "support/claude_native.rs"]
mod claude_native;
use claude_native::{Fixture, Result, Scenario, require, tls_arguments, write_evidence};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};
const MARKER: &str = "SHINATAMA_SYNTHETIC_CLAUDE_TLS_OK";
#[test]
#[ignore = "root must review fixed local Claude TLS recipe before explicitly scheduling this native proof"]
fn installed_claude_bun_accepts_only_private_child_ca_through_canonical_proxy() {
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
    let mut fixture = Fixture::prepare(Scenario::Tls)?;
    let observed = observe(&mut fixture);
    // Release every exact owner even when a case fails, before propagating it.
    fixture.release_all()?;
    let mut evidence = fixture.evidence(observed?)?;
    evidence["parentTlsEnvironmentUnchanged"] = true.into();
    write_evidence("claude-proxy-tls.json", &evidence)
}
fn observe(fixture: &mut Fixture) -> Result<Value> {
    fixture.start_sidecar()?;
    for private_ca in [false, true] {
        let ready = fixture.phase("case-ready")?;
        require(
            ready["privateCa"].as_bool() == Some(private_ca),
            "ordered-native-claude-cases",
        )?;
        let case = fixture.create_case(if private_ca { "private-ca" } else { "no-ca" })?;
        let command =
            fixture.claude_command(&case, private_ca, tls_arguments(), BTreeMap::new())?;
        let index = fixture.start_claude(command)?;
        let exit = fixture.wait_exit(index, Duration::from_secs(11))?;
        let (output, errors) = fixture.output(index)?;
        let output = String::from_utf8_lossy(output);
        let errors = String::from_utf8_lossy(errors);
        let marker_observed = output.contains(MARKER);
        let certificate_refusal = [output.as_ref(), errors.as_ref()].iter().any(|text| {
            let lower = text.to_ascii_lowercase();
            [
                "certificate",
                "cert_",
                "unable to verify",
                "self-signed",
                "self signed",
            ]
            .iter()
            .any(|needle| lower.contains(needle))
        });
        fixture.release(index)?;
        fixture.send(
            json!({"privateCa":private_ca,"exitCode":exit,"markerObserved":marker_observed,
            "certificateRefusal":certificate_refusal,"outputBounded":true,"ownerReleased":true}),
        )?;
        let record = fixture.phase("case-finished")?;
        require(
            record["privateCa"].as_bool() == Some(private_ca),
            "matching-native-case-receipt",
        )?;
    }
    fixture.shutdown()
}
