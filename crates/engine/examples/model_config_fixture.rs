//! Emit the real launcher translations for the loopback-only provider smoke test.
use shinatama_engine::model_config::{Configuration, Snapshot};
use std::io::Read;

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    let configuration: Configuration = serde_json::from_str(&input).unwrap();
    configuration.validate().unwrap();
    let snapshot = Snapshot {
        configuration,
        ..Snapshot::default()
    };
    println!(
        "{}",
        serde_json::json!({
            "pi": snapshot.pi_extension(),
            "opencode": snapshot.opencode(serde_json::json!({}))
        })
    );
}
