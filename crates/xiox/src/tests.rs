use super::*;
use crate::observer::read_line;
use std::{
    io::Cursor,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const STARTED: &str = r#"{"schema":1,"event":"started","time":"2026-11-13T20:30:52.871Z","project":{"root":"C:\\w\\sampleapp","name":"sampleapp","isRepo":true,"origin":null,"worktrees":["C:\\w\\sampleapp-feature"]},"params":{"growThreshold":2147483648,"worktreeThreshold":3},"totals":{"byCategory":{"builds":3145772},"items":2,"recommendedBytes":3145944}}"#;
const CHANGED: &str = r#"{"schema":1,"event":"changed","time":"2026-11-13T20:30:22.707Z","added":[{"key":"c:\\w\\sampleapp-spike1","category":"worktrees","path":"C:\\w\\sampleapp-spike1","bytes":171,"level":"recommended","blocked":[],"why":{"type":"linked-worktree","branch":"spike1","head":"f0aa"},"target":{"type":"worktree","repo":"C:\\w\\sampleapp"},"new":true,"firstSeenMs":1794601822706,"measuredAtMs":1794601822706,"bytesPartial":false},{"key":"c:\\w\\sampleapp-bench","category":"named","path":"C:\\w\\sampleapp-bench","bytes":0,"level":"caution","blocked":[],"why":{"type":"name-only"},"target":{"type":"dir","kind":"project-folder"},"provenance":null,"new":true,"firstSeenMs":1794601822706,"measuredAtMs":1794601822706,"bytesPartial":false},{"key":"c:\\w\\sampleapp-copy","category":"copies","path":"C:\\w\\sampleapp-copy","bytes":4096,"level":"caution","blocked":["unsaved-work"],"why":{"type":"same-repository","sharedRootCommit":true,"sameOrigin":true,"hasOwnWork":true},"target":{"type":"dir","kind":"project-folder"},"new":true,"firstSeenMs":1794601822706,"measuredAtMs":null,"bytesPartial":true}],"removed":[],"growthBytes":2097494}"#;
const ATTENTION: &str = r#"{"schema":1,"event":"attention","time":"2026-11-13T20:30:22.709Z","kind":"worktrees","value":4,"threshold":3,"cleanCommand":"xiox clean \"C:\\w\\sampleapp\" --include \"C:\\w\\sampleapp-spike1\""}"#;
const REMOVED: &str = r#"{"schema":1,"event":"changed","time":"2026-11-13T20:30:32.967Z","added":[],"removed":["c:\\w\\sampleapp-spike1"],"growthBytes":2097323}"#;
const REPORT: &str = r#"{"schema":1,"event":"report","time":"2026-11-13T20:31:00.000Z","project":{"root":"C:\\w\\sampleapp","name":"sampleapp","isRepo":true,"origin":null,"worktrees":[]},"items":[{"key":"c:\\w\\sampleapp\\target","category":"builds","path":"C:\\w\\sampleapp\\target","bytes":3145772,"level":"recommended","blocked":[],"why":{"type":"catalog-folder","labelEn":"Rust build"},"target":{"type":"dir","kind":"rust-target"},"new":false,"firstSeenMs":1,"measuredAtMs":2,"bytesPartial":false},{"key":"session:claude:abc","category":"sessions","path":null,"bytes":null,"level":"optional","blocked":[],"why":{"type":"archived-session","agent":"claude","title":"Bench the parser"},"target":{"type":"session","agent":"claude","id":"abc"},"new":true,"firstSeenMs":5,"measuredAtMs":null,"bytesPartial":false},{"key":"c:\\users\\a\\scratch\\bench-1","category":"scratch","path":"C:\\Users\\a\\scratch\\bench-1","bytes":10,"level":"caution","blocked":[],"why":{"type":"session-folder","strength":"created","fromScript":false,"risks":[],"activity":"unknown"},"target":{"type":"dir","kind":"project-folder"},"provenance":{"strength":"created","lastActivityMs":6,"sessions":[{"agent":"codex","harness":"codex","store":"s","id":"019a-bench","strength":"created","listedCwd":false,"operations":[],"omitted":0,"failed":0,"gaps":false}]},"new":true,"firstSeenMs":6,"measuredAtMs":6,"bytesPartial":false}],"notes":[],"branches":[],"search":"bounded-walk","scannedAtMs":7,"elapsedMs":8,"totals":{"byCategory":{},"items":3,"recommendedBytes":0},"growthBytes":10,"attention":[],"stale":false,"cleanCommand":null,"cleanCommandTruncated":false,"startedAtMs":1794601865194,"checkedAtMs":1794601871137}"#;

#[test]
fn started_then_changed_keeps_only_what_appeared() {
    let mut picture = Picture::default();
    assert_eq!(picture.phase, Phase::Starting);
    assert_eq!(picture.apply(STARTED), Ok(Change::Started));
    assert_eq!(picture.phase, Phase::Watching);
    assert_eq!(picture.root.as_deref(), Some(r"C:\w\sampleapp"));
    assert!(picture.is_repo);
    assert!(picture.started_at_ms.is_some());
    assert!(
        picture.new_items.is_empty(),
        "the snapshot is the project as it was, not a leftover"
    );

    assert_eq!(
        picture.apply(CHANGED),
        Ok(Change::Changed {
            added: vec![
                r"c:\w\sampleapp-spike1".into(),
                r"c:\w\sampleapp-bench".into(),
                r"c:\w\sampleapp-copy".into()
            ]
        })
    );
    assert_eq!(picture.growth_bytes, 2_097_494);
    let reasons: Vec<&str> = picture
        .new_items
        .iter()
        .map(|item| item.reason.as_str())
        .collect();
    assert_eq!(
        reasons,
        [
            "worktree on branch spike1",
            "named like the project",
            "copy of the repository, with its own work"
        ]
    );
    let copy = &picture.new_items[2];
    assert_eq!(
        copy.blocked,
        ["has commits or changes the project does not have"]
    );
    assert!(copy.bytes_partial);

    // The same line again lists nothing new: items are replaced by key.
    assert_eq!(
        picture.apply(CHANGED),
        Ok(Change::Changed { added: vec![] })
    );
    assert_eq!(picture.new_items.len(), 3);

    assert_eq!(
        picture.apply(REMOVED),
        Ok(Change::Changed { added: vec![] })
    );
    assert_eq!(picture.new_items.len(), 2);
    assert!(
        picture
            .new_items
            .iter()
            .all(|item| item.key != r"c:\w\sampleapp-spike1")
    );
}

#[test]
fn attention_records_the_threshold_and_the_clean_command() {
    let mut picture = Picture::default();
    picture.apply(STARTED).unwrap();
    assert_eq!(
        picture.apply(ATTENTION),
        Ok(Change::Attention {
            kind: "worktrees".into(),
            value: 4
        })
    );
    picture.apply(ATTENTION).unwrap();
    assert_eq!(picture.attention, ["worktrees"], "a kind is listed once");
    assert!(
        picture
            .clean_command
            .as_deref()
            .unwrap()
            .starts_with("xiox clean ")
    );
}

#[test]
fn a_report_replaces_the_picture() {
    let mut picture = Picture::default();
    picture.apply(STARTED).unwrap();
    picture.apply(CHANGED).unwrap();
    picture.apply(ATTENTION).unwrap();
    assert_eq!(picture.apply(REPORT), Ok(Change::Report));
    let keys: Vec<&str> = picture
        .new_items
        .iter()
        .map(|item| item.key.as_str())
        .collect();
    assert_eq!(keys, ["session:claude:abc", r"c:\users\a\scratch\bench-1"]);
    let session = &picture.new_items[0];
    assert_eq!(session.label.as_deref(), Some("Bench the parser"));
    assert_eq!(session.reason, "archived claude session");
    assert_eq!(session.path, None);
    assert_eq!(picture.new_items[1].reason, "created by an agent session");
    assert_eq!(
        picture.new_items[1].sessions,
        [SessionRef {
            harness: "codex".into(),
            id: "019a-bench".into(),
            strength: Some("created".into())
        }]
    );
    assert!(picture.new_items[0].sessions.is_empty());
    assert!(picture.attention.is_empty());
    assert_eq!(picture.clean_command, None);
    assert_eq!(picture.started_at_ms, Some(1_794_601_865_194));
    assert_eq!(picture.checked_at_ms, Some(1_794_601_871_137));
}

#[test]
fn errors_are_kept_and_unknown_lines_ignored() {
    let mut picture = Picture::default();
    let error =
        r#"{"schema":1,"event":"error","time":"t","message":"check failed: path not found"}"#;
    assert_eq!(picture.apply(error), Ok(Change::Error));
    assert_eq!(
        picture.message.as_deref(),
        Some("check failed: path not found")
    );
    assert_eq!(
        picture.apply(r#"{"schema":1,"event":"later","time":"t"}"#),
        Ok(Change::Ignored)
    );
    assert_eq!(
        picture.apply(r#"{"schema":2,"event":"report"}"#),
        Ok(Change::Ignored)
    );
    assert!(picture.apply("not json").is_err());
    assert!(
        picture
            .apply(r#"{"schema":1,"event":"changed","added":"no"}"#)
            .is_err()
    );
}

#[test]
fn new_items_are_bounded() {
    let mut picture = Picture::default();
    let items: Vec<String> = (0..MAX_NEW_ITEMS + 5)
        .map(|n| format!(r#"{{"key":"k{n}","category":"named","path":"p{n}","level":"caution","why":{{"type":"name-only"}},"new":true}}"#))
        .collect();
    let line = format!(
        r#"{{"schema":1,"event":"changed","added":[{}],"removed":[],"growthBytes":0}}"#,
        items.join(",")
    );
    assert!(
        matches!(picture.apply(&line), Ok(Change::Changed { added }) if added.len() == MAX_NEW_ITEMS)
    );
    assert_eq!(picture.new_items.len(), MAX_NEW_ITEMS);
    assert_eq!(picture.new_items_omitted, 5);
}

#[test]
fn lines_over_the_bound_are_skipped_whole() {
    let mut reader = Cursor::new(b"short\nxxxxxxxxxxxxxxxxxxxx\nlast".to_vec());
    let mut line = Vec::new();
    assert_eq!(read_line(&mut reader, &mut line, 8).unwrap(), Some(true));
    assert_eq!(line, b"short");
    assert_eq!(read_line(&mut reader, &mut line, 8).unwrap(), Some(false));
    assert!(line.is_empty());
    assert_eq!(read_line(&mut reader, &mut line, 8).unwrap(), Some(true));
    assert_eq!(line, b"last");
    assert_eq!(read_line(&mut reader, &mut line, 8).unwrap(), None);
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("shinatama-xiox-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_variable_wins_and_off_switches_off() {
    let dir = scratch("variable");
    let program = dir.join(EXE);
    std::fs::write(&program, b"").unwrap();
    assert_eq!(locate_from(Some("off".into()), None, None), Program::Off);
    assert_eq!(locate_from(Some(" OFF ".into()), None, None), Program::Off);
    assert_eq!(
        locate_from(Some(program.clone().into()), None, None),
        Program::Found(program.clone())
    );
    assert!(matches!(
        locate_from(Some(dir.join("none").into()), None, None),
        Program::Missing(_)
    ));
    // An empty variable is as if it were not set.
    assert!(matches!(
        locate_from(Some("".into()), None, None),
        Program::Missing(_)
    ));
}

#[test]
fn beside_the_program_then_one_up_then_path() {
    let dir = scratch("beside");
    std::fs::create_dir_all(dir.join("cli")).unwrap();
    std::fs::create_dir_all(dir.join("xiox")).unwrap();
    let bundled = dir.join("xiox").join(EXE);
    std::fs::write(&bundled, b"").unwrap();
    let app = dir.join("Shinatama.exe");
    let cli = dir.join("cli").join("shinatama.exe");
    assert_eq!(
        locate_from(None, Some(app), None),
        Program::Found(bundled.clone())
    );
    assert_eq!(locate_from(None, Some(cli), None), Program::Found(bundled));

    let elsewhere = scratch("path");
    let on_path = elsewhere.join(EXE);
    std::fs::write(&on_path, b"").unwrap();
    let path = std::env::join_paths([elsewhere.clone()]).unwrap();
    assert_eq!(
        locate_from(None, Some(elsewhere.join("app")), Some(path)),
        Program::Found(on_path)
    );
    assert!(matches!(locate_from(None, None, None), Program::Missing(_)));
}

/// Collects what the listener saw, to wait on it.
#[derive(Clone, Default)]
struct Seen(Arc<Mutex<Vec<(Update, Picture)>>>);
impl Seen {
    fn listener(&self) -> impl Fn(&Picture, &Update) + Send + Sync + 'static {
        let seen = self.0.clone();
        move |picture, update| seen.lock().unwrap().push((update.clone(), picture.clone()))
    }
    fn wait_for(&self, wanted: impl Fn(&Update) -> bool, limit: Duration) -> Picture {
        let deadline = Instant::now() + limit;
        loop {
            if let Some((_, picture)) = self
                .0
                .lock()
                .unwrap()
                .iter()
                .find(|(update, _)| wanted(update))
            {
                return picture.clone();
            }
            assert!(
                Instant::now() < deadline,
                "the observer did not send the expected update"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// A stand-in for `xiox observe` that speaks the same lines.
#[cfg(unix)]
fn fake_observer(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let report = REPORT.replace('\'', "'\\''");
    let script = format!(
        "#!/bin/sh\n[ \"$1\" = observe ] || exit 2\necho 'Observing' >&2\nprintf '%s\\n' '{STARTED}'\nwhile read line; do\n  case \"$line\" in\n    report) printf '%s\\n' '{report}';;\n    stop) exit 0;;\n  esac\ndone\n"
    );
    let program = dir.join("fake-xiox");
    std::fs::write(&program, script).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    program
}

#[cfg(unix)]
#[test]
fn the_process_reports_on_request_and_stops() {
    let dir = scratch("process");
    let seen = Seen::default();
    let observer = Observer::spawn(
        &fake_observer(&dir),
        &dir,
        Options::default(),
        seen.listener(),
    )
    .unwrap();
    seen.wait_for(
        |update| *update == Update::Line(Change::Started),
        Duration::from_secs(10),
    );
    assert!(observer.report());
    let picture = seen.wait_for(
        |update| *update == Update::Line(Change::Report),
        Duration::from_secs(10),
    );
    assert_eq!(picture.new_items.len(), 2);
    observer.stop();
    let ended = seen.wait_for(|update| *update == Update::Ended, Duration::from_secs(10));
    assert_eq!(ended.phase, Phase::Ended);
    assert_eq!(ended.message, None, "a requested stop is not a failure");
    assert!(observer.ended());
    assert!(!observer.report());
}

#[cfg(unix)]
#[test]
fn an_observer_that_fails_says_why() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("fails");
    let program = dir.join("failing-xiox");
    std::fs::write(
        &program,
        "#!/bin/sh\necho 'error: path not found: /nowhere' >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let seen = Seen::default();
    let _observer = Observer::spawn(&program, &dir, Options::default(), seen.listener()).unwrap();
    let ended = seen.wait_for(|update| *update == Update::Ended, Duration::from_secs(10));
    assert_eq!(
        ended.message.as_deref(),
        Some("error: path not found: /nowhere")
    );
}

#[cfg(unix)]
#[test]
fn the_observer_is_given_shinatamas_mukade() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("mukade");
    let program = dir.join("echoing-xiox");
    std::fs::write(
        &program,
        "#!/bin/sh\necho \"mukade: $XIOX_MUKADE_EXE\" >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let seen = Seen::default();
    let options = Options {
        mukade: Some(PathBuf::from("/app/mukade/mukade")),
        ..Options::default()
    };
    let _observer = Observer::spawn(&program, &dir, options, seen.listener()).unwrap();
    let ended = seen.wait_for(|update| *update == Update::Ended, Duration::from_secs(10));
    assert_eq!(ended.message.as_deref(), Some("mukade: /app/mukade/mukade"));
}

/// Against the real program: `XIOX_TEST_PROGRAM=<path to xiox> cargo test -p shinatama-xiox`
/// (`XIOX_TEST_MUKADE=<path to mukade>` passes that Mukade, as Shinatama passes its own).
/// A repository is created, then a worktree and a bench folder while the observer runs.
#[test]
fn the_real_observer_sees_a_new_worktree_and_a_bench_folder() {
    let Some(program) = std::env::var_os("XIOX_TEST_PROGRAM").map(PathBuf::from) else {
        eprintln!("XIOX_TEST_PROGRAM is not set; the real observer was not run.");
        return;
    };
    let dir = scratch("real");
    let project = dir.join("sampleapp");
    std::fs::create_dir_all(&project).unwrap();
    let git = |args: &[&str], cwd: &Path| {
        let status = std::process::Command::new("git")
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .current_dir(cwd)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    };
    git(&["init", "-q"], &project);
    git(&["commit", "-q", "--allow-empty", "-m", "init"], &project);
    let seen = Seen::default();
    let options = Options {
        interval_secs: 60,
        no_drives: true,
        mukade: std::env::var_os("XIOX_TEST_MUKADE").map(PathBuf::from),
    };
    let observer = Observer::spawn(&program, &project, options, seen.listener()).unwrap();
    seen.wait_for(
        |update| *update == Update::Line(Change::Started),
        Duration::from_secs(60),
    );
    git(&["worktree", "add", "-q", "../sampleapp-spike1"], &project);
    std::fs::create_dir_all(dir.join("sampleapp-bench")).unwrap();
    assert!(observer.report());
    let picture = seen.wait_for(
        |update| *update == Update::Line(Change::Report),
        Duration::from_secs(60),
    );
    let mut categories: Vec<&str> = picture
        .new_items
        .iter()
        .map(|item| item.category.as_str())
        .collect();
    categories.sort_unstable();
    assert_eq!(categories, ["named", "worktrees"], "{picture:#?}");
    observer.stop();
    let ended = seen.wait_for(|update| *update == Update::Ended, Duration::from_secs(10));
    assert_eq!(ended.message, None);
}
