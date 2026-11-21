//! F03H: hop uniqueness and recovery across real processes on one journal file.
//!
//! The test re-runs its own test binary as child processes (`HOP_CHILD=plan|recover`), so the
//! race and the recovery happen in separate processes holding separate SQLite connections.
use shinatama_state::hops::{
    HopJournal, HopMode, HopPlan, HopRecord, HopState, HopStrategy, RecoveryProbe,
};
use std::path::PathBuf;
use std::process::Command;

fn plan(id: char) -> HopPlan {
    HopPlan {
        id: std::iter::repeat_n(id, 32).collect(),
        thread_id: "t_crossproc".into(),
        destination_thread_id: None,
        mode: HopMode::Close,
        strategy: HopStrategy::SeedPrompt,
        source_agent: "claude_code".into(),
        source_native_id: "native".into(),
        source_instance: Some("i_src".into()),
        destination_agent: "codex".into(),
        nonce: "n".into(),
        snapshot_sha256: "a".repeat(64),
        bootstrap_sha256: "b".repeat(64),
        seed_path: "C:/seed.md".into(),
    }
}

struct Dead;
impl RecoveryProbe for Dead {
    fn destination_alive(&mut self, _: &HopRecord) -> bool {
        false
    }
}

/// Child entry points, selected by environment variable.
#[test]
fn child() {
    let Ok(role) = std::env::var("HOP_CHILD") else {
        return;
    };
    let path = PathBuf::from(std::env::var("HOP_JOURNAL").unwrap());
    let journal = HopJournal::open_path(&path).unwrap();
    match role.as_str() {
        "plan" => {
            let id = std::env::var("HOP_ID").unwrap().chars().next().unwrap();
            let barrier = PathBuf::from(std::env::var("HOP_BARRIER").unwrap());
            while !barrier.exists() {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            let outcome = journal.plan(&plan(id), 1);
            println!(
                "HOP_RESULT={}",
                if outcome.is_ok() { "won" } else { "lost" }
            );
        }
        "recover" => {
            let done = journal.recover(&mut Dead, 1_000).unwrap();
            println!(
                "HOP_RESULT={}",
                done.iter().filter(|r| r.to.is_some()).count()
            );
        }
        _ => {}
    }
}

fn run_child(
    role: &str,
    journal: &std::path::Path,
    extra: &[(&str, String)],
) -> std::process::Child {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "child", "--nocapture", "--test-threads=1"])
        .env("HOP_CHILD", role)
        .env("HOP_JOURNAL", journal)
        .stdout(std::process::Stdio::piped());
    for (k, v) in extra {
        command.env(k, v);
    }
    command.spawn().unwrap()
}

fn result(child: std::process::Child) -> String {
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|l| {
            l.split_once("HOP_RESULT=")
                .map(|(_, v)| v.split_whitespace().next().unwrap_or_default().to_owned())
        })
        .unwrap_or_default()
}

#[test]
fn six_processes_race_and_exactly_one_wins_then_recovery_runs_once() {
    if std::env::var("HOP_CHILD").is_ok() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("shinatama-crossproc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let journal_path = dir.join("hops.sqlite");
    HopJournal::open_path(&journal_path).unwrap();
    let barrier = dir.join("go");
    let children: Vec<_> = "123456"
        .chars()
        .map(|id| {
            run_child(
                "plan",
                &journal_path,
                &[
                    ("HOP_ID", id.to_string()),
                    ("HOP_BARRIER", barrier.display().to_string()),
                ],
            )
        })
        .collect();
    std::thread::sleep(std::time::Duration::from_millis(400));
    std::fs::write(&barrier, b"go").unwrap();
    let outcomes: Vec<String> = children.into_iter().map(result).collect();
    assert_eq!(
        outcomes.iter().filter(|o| *o == "won").count(),
        1,
        "{outcomes:?}"
    );
    assert_eq!(
        outcomes.iter().filter(|o| *o == "lost").count(),
        5,
        "{outcomes:?}"
    );
    // Advance the winner to SWITCHING, then "crash" (drop) and recover from two processes.
    let journal = HopJournal::open_path(&journal_path).unwrap();
    let winner = journal.active_for_thread("t_crossproc").unwrap().unwrap();
    let mut hop = winner;
    for next in [
        HopState::Previewed,
        HopState::Confirmed,
        HopState::Preparing,
        HopState::Ready,
        HopState::Switching,
    ] {
        hop = journal
            .transition(&hop.plan.id, hop.state, hop.revision, next, None, 0)
            .unwrap();
    }
    drop(journal);
    let recoverers = [
        run_child("recover", &journal_path, &[]),
        run_child("recover", &journal_path, &[]),
    ];
    let counts: u32 = recoverers
        .into_iter()
        .map(|c| result(c).parse::<u32>().unwrap())
        .sum();
    assert_eq!(counts, 1, "exactly one process applied the recovery");
    let journal = HopJournal::open_path(&journal_path).unwrap();
    assert_eq!(
        journal.hop(&hop.plan.id).unwrap().unwrap().state,
        HopState::Aborted,
        "dead destination: the source is reopened"
    );
    assert!(journal.integrity_check().unwrap());
    assert_eq!(
        journal
            .events(&hop.plan.id)
            .unwrap()
            .iter()
            .filter(|e| e.to_state == Some(HopState::Aborted))
            .count(),
        1
    );
    drop(journal);
    std::fs::remove_dir_all(dir).unwrap();
}
