//! H27 driver: the four calls technical design 5.4.1 says Shinatama makes, as a CLI.
//! Store roots come from the agents' own variables (CLAUDE_CONFIG_DIR, CODEX_HOME,
//! PI_CODING_AGENT_DIR, OPENCODE_DB), so every run stays in a scratch home.
//!
//!   h27-driver list  <harness>
//!   h27-driver dump  <harness> <session-id-prefix>          → Common JSON on stdout
//!   h27-driver write <harness> <common.json> [--root DIR]   → {"id","location"}

use anyhow::{Context, Result, anyhow, bail};
use serde_json::json;
use std::path::PathBuf;
use txcript::common::{Message, Meta};
use txcript::local::{self, Session, WriteOpts};
use txcript::{HarnessId, Transcript};

fn harness(name: &str) -> Result<HarnessId> {
    name.parse::<HarnessId>().map_err(|_| anyhow!("unknown harness {name}"))
}

fn find(h: HarnessId, id: &str) -> Result<Session> {
    let mut hits: Vec<Session> = local::discover_harness(h)?
        .into_iter()
        .filter(|s| s.meta.id.starts_with(id))
        .collect();
    match hits.len() {
        1 => Ok(hits.remove(0)),
        0 => bail!("no {h:?} session with id prefix {id}"),
        n => bail!("{n} {h:?} sessions match {id}"),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("list") => {
            let h = harness(&args[1])?;
            for s in local::discover_harness(h)? {
                println!("{}", json!({"id": s.meta.id, "title": s.meta.title, "cwd": s.meta.cwd,
                    "timestamp": s.meta.timestamp, "location": s.location()}));
            }
        }
        Some("dump") => {
            let t = find(harness(&args[1])?, &args[2])?.read()?;
            println!("{}", serde_json::to_string_pretty(&json!({"meta": t.meta, "messages": t.body}))?);
        }
        Some("write") => {
            let h = harness(&args[1])?;
            let doc: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&args[2]).context("read common json")?)?;
            let meta: Meta = serde_json::from_value(doc["meta"].clone()).context("meta")?;
            let messages: Vec<Message> = serde_json::from_value(doc["messages"].clone()).context("messages")?;
            let root = args.iter().position(|a| a == "--root").map(|i| PathBuf::from(&args[i + 1]));
            let w = local::write_with(h, &Transcript::new(meta, messages), WriteOpts { root: root.as_deref(), metadata: None })?;
            let (cmd, argv) = local::resume_command(h, &w.id);
            println!("{}", json!({"id": w.id, "location": w.location, "resume": [cmd, argv]}));
        }
        _ => bail!("usage: h27-driver list|dump|write …"),
    }
    Ok(())
}
