#![cfg(windows)]
//! SC9: control messages pass through the pinned sidecar byte-identical. A synthetic
//! local origin (fixtures/origin.mjs) stands in for the model provider: the sidecar's
//! only upstream is that loopback server, so no paid call can happen. The test client
//! sends an Anthropic Messages request through the running sidecar and reads back, from
//! the origin's own recording, what the sidecar actually forwarded.
//!
//! The harness below (Running, Origin, Msg, last_user_text_at_origin, Origin::bodies) is
//! meant to be reused by the session-isolation, drain and soak proofs.
#[path = "support/sidecar_harness.rs"]
mod sidecar_harness;
use sidecar_harness::*;

use serde_json::{Value, json};
use shinatama_engine::sidecar_supervisor::{Command, Phase, Reason};
use std::{
    collections::BTreeMap,
    fs,
    io::{ErrorKind, Read},
    net::TcpStream,
    path::PathBuf,
    process::Stdio,
    thread,
    time::{Duration, Instant},
};

// SC9 outcome against the pinned sidecar (all four fail, assertions intact; Task 12 owns the fix):
// - the human message reaches the origin as `<acp tokens="N" type="text">mNNNNN</acp>` and a newline
//   before the original text: a reference tag is prepended inside the same text block;
// - the first destination turn makes the sidecar fetch the model catalog (GET /v1/models) and
//   send the control message to a lightweight model for rewriting (a second POST) before the
//   probe itself is forwarded.
#[test]
#[ignore = "SC9 open: see Task 12"]
fn bootstrap_reaches_origin_byte_identical() {
    let (sidecar, origin) = proof();
    let text = handoff_text();
    assert_eq!(
        last_user_text_at_origin(&sidecar, &origin, &[], &text),
        text
    );
}

/// A conversation whose tool results are each larger than the 4000-token compression
/// threshold and together sum to about 150_000 estimated tokens (four bytes per token),
/// just under the 160_000 adaptive-pressure ceiling of the baseline.
fn long_history() -> Vec<Msg> {
    const RESULTS: usize = 30;
    const TOKENS_EACH: usize = 5_000;
    let mut seed = 0x2545_f491_u64;
    let mut word = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        format!("{:x}", seed >> 24)
    };
    let mut history = vec![
        Msg::user_text("Analizza il repository e riassumi cosa trovi file per file."),
        Msg::assistant_text("Parto leggendo i file uno alla volta."),
    ];
    for index in 0..RESULTS {
        let id = format!("toolu_{index:04}");
        let mut result = format!("file {index}\n");
        while result.len() < TOKENS_EACH * 4 {
            result.push_str(&word());
            result.push(' ');
        }
        history.push(Msg::assistant_tool_use(
            &id,
            "Read",
            json!({ "file_path": format!("C:\\synthetic\\src\\file{index}.rs") }),
        ));
        history.push(Msg::tool_result(&id, &result));
    }
    history.push(Msg::assistant_text("Ho letto tutti i file."));
    history
}

#[test]
#[ignore = "SC9 open: see Task 12"]
fn bootstrap_is_untouched_with_history_near_the_limit() {
    let history = long_history();
    let tokens: usize = history
        .iter()
        .map(|m| m.content.to_string().len() / 4)
        .sum();
    assert!(
        (140_000..160_000).contains(&tokens),
        "history must sit near the limit, estimated {tokens}"
    );
    let (sidecar, origin) = proof();
    let text = handoff_text();
    assert_eq!(
        last_user_text_at_origin(&sidecar, &origin, &history, &text),
        text
    );
}

#[test]
#[ignore = "SC9 open: see Task 12"]
fn first_destination_turn_sends_no_auxiliary_request() {
    let (sidecar, origin) = proof();
    let text = handoff_text();
    let forwarded =
        forward(sidecar.port, &origin, &new_session(), &[], &text).expect("forward the first turn");
    // A catalog fetch is allowed; a rewrite or summary call (another model call) is not.
    assert_eq!(
        forwarded.model_calls, 1,
        "requests besides the probe: {:?}",
        forwarded.others
    );
    // A late auxiliary call would still be on its way: give it the chance to arrive.
    thread::sleep(Duration::from_millis(1500));
    // origin directory holds exactly one model-call body after the turn: no rewrite call,
    // no summary call
    assert_eq!(origin.bodies().len(), 1);
    let only = &origin.requests()[0];
    assert_eq!(only.method, "POST");
    assert!(only.path.ends_with("/v1/messages"), "{}", only.path);
}

const DICTATED: &str = "Vorrei che sistemassi il calcolo dei totali nel report mensile, ma non toccare per nessun motivo la parte sugli arrotondamenti, perché i clienti si aspettano sempre 2 decimali; poi dimmi quante righe hai cambiato e se i test esistenti passano.";

#[test]
#[ignore = "SC9 open: see Task 12"]
fn dictated_prompt_is_kept_and_any_rewrite_note_is_additive() {
    assert_eq!(DICTATED.split_whitespace().count(), 40);
    assert!(DICTATED.contains("non toccare") && DICTATED.contains("2 decimali"));
    let (sidecar, origin) = proof();
    let history = [
        Msg::user_text("Ciao, lavoriamo sul report mensile."),
        Msg::assistant_text("Va bene, dimmi cosa serve."),
    ];
    let forwarded = forward(sidecar.port, &origin, &new_session(), &history, DICTATED)
        .expect("forward dictation");
    let (at, human) = human_message(&forwarded).expect("human message");
    // Contract: the original text reaches the origin byte-identical and as a text block of
    // its own. Anything the sidecar adds is a derived rewrite note, placed adjacent to the
    // original either as a separate text block of the same message or as a separate message
    // right after it; the note says the original stays authoritative, and it is never text
    // spliced into the original block.
    let blocks = message_text(human);
    let original = blocks
        .iter()
        .position(|block| block == DICTATED)
        .unwrap_or_else(|| panic!("no text block equals the original; blocks: {blocks:?}"));
    let mut notes: Vec<String> = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        if index != original {
            assert!(
                is_note_text(block),
                "block {index} is not a derived note: {block}"
            );
            notes.push(block.clone());
        }
    }
    let after = &forwarded.messages[at + 1..];
    assert!(
        after.iter().all(is_note),
        "only derived notes may follow the original message"
    );
    notes.extend(after.iter().flat_map(message_text));
    for note in &notes {
        assert!(
            note.contains("The original user message remains authoritative"),
            "{note}"
        );
    }
    // The history keeps its messages; the sidecar adds nothing before the original.
    assert_eq!(forwarded.messages[..at].len(), history.len());
}

// The origin handle must be shareable so concurrent sessions can forward from threads.
const _: fn() = || {
    fn sync<T: Sync>() {}
    sync::<Origin>();
};

// Session isolation and resumed identity (Task 7). Sessions are told apart at the origin by
// the `x-claude-code-session-id` header: the sidecar copies the client's headers onto its
// auxiliary requests (rewrite, summary), so every model call it makes on a session's behalf
// carries that session's id and is attributed to it.
const SESSION_HEADER: &str = "x-claude-code-session-id";
const TURNS: usize = 20;

/// The same ten messages for every session: five question and answer pairs.
fn shared_prefix() -> Vec<Msg> {
    (0..5)
        .flat_map(|n| {
            [
                Msg::user_text(&format!(
                    "Domanda comune numero {n}: descrivi il modulo {n}."
                )),
                Msg::assistant_text(&format!("Il modulo {n} fa una cosa sola, e la fa bene.")),
            ]
        })
        .collect()
}
/// The shared prefix and the assistant's tool use, then, separately, the tool result of
/// about 6000 tokens (four bytes per token, above the 4000-token CCR threshold) that opens
/// and closes with `marker`. The filler is the same for every session, so the marker is
/// the only difference between two sessions' tool results.
fn session_start(marker: &str) -> (Vec<Msg>, Msg) {
    let mut seed = 0x9e37_79b9_u64;
    let mut result = format!("{marker}\n");
    while result.len() < 6_000 * 4 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        result.push_str(&format!("{:x} ", seed >> 24));
    }
    result.push_str(marker);
    let mut history = shared_prefix();
    history.push(Msg::assistant_tool_use(
        "toolu_marker",
        "Read",
        json!({ "file_path": r"C:\synthetic\marker.txt" }),
    ));
    (history, Msg::tool_result("toolu_marker", &result))
}
/// First request of a session: the newest message is the large tool result answering the
/// tool use, as an agent sends it. Returns the conversation as it stands afterwards.
fn open_session(
    port: u16,
    origin: &Origin,
    session: &str,
    label: &str,
    marker: &str,
) -> ProofResult<Vec<Msg>> {
    let (mut history, result) = session_start(marker);
    let mut messages: Vec<Value> = history.iter().map(Msg::wire).collect();
    messages.push(result.wire());
    forward_messages(port, origin, session, messages, Some(&read_tool()))
        .map_err(|e| format!("{label} opening request: {e}"))?;
    history.push(result);
    history.push(Msg::assistant_text("Ho letto il file."));
    Ok(history)
}
/// The turn on which the origin asks the sidecar for the stored original.
const RETRIEVE_TURN: usize = 3;
/// Runs `turns` turns on `session`, each one resent with the whole conversation so far.
/// On `RETRIEVE_TURN` the user text carries the word that makes the origin ask for the
/// stored original through the sidecar's retrieve tool.
fn run_turns(
    port: u16,
    origin: &Origin,
    session: &str,
    history: &mut Vec<Msg>,
    label: &str,
    turns: std::ops::Range<usize>,
) -> ProofResult<()> {
    for turn in turns {
        let ask = if turn == RETRIEVE_TURN {
            " RETRIEVE_NOW"
        } else {
            ""
        };
        let text = format!("Turno {turn} della sessione {label}: continua il lavoro.{ask}");
        forward_with(port, origin, session, history, &text, Some(&read_tool()))
            .map_err(|e| format!("{label} turn {turn}: {e}"))?;
        history.push(Msg::user_text(&text));
        history.push(Msg::assistant_text(REPLY));
    }
    Ok(())
}
/// Every model call the origin saw, grouped by the session header it carries. A call
/// without the header, or from an unexpected session, fails the attribution.
fn model_calls_by_session(
    origin: &Origin,
    sessions: &[&str],
) -> ProofResult<BTreeMap<String, Vec<Recorded>>> {
    let mut grouped: BTreeMap<String, Vec<Recorded>> = BTreeMap::new();
    for call in origin.requests() {
        let id = call.headers[SESSION_HEADER].as_str().unwrap_or_default();
        if !sessions.contains(&id) {
            return Err(format!(
                "a model call is not attributable to a session: header {id:?}"
            ));
        }
        grouped.entry(id.to_owned()).or_default().push(call);
    }
    Ok(grouped)
}
fn count_in(calls: &[Recorded], needle: &str) -> usize {
    calls
        .iter()
        .filter(|c| String::from_utf8_lossy(&c.body).contains(needle))
        .count()
}
/// `sessions` of `/__bili/stats`, each as (id, requests).
fn stats(port: u16) -> ProofResult<Vec<(String, u64)>> {
    let response = request(port, "GET", "/__bili/stats", &[], b"")?;
    check(response.status == 200, "stats not available")?;
    let value: Value = safe(serde_json::from_slice(&response.body), "invalid stats JSON")?;
    Ok(value["sessions"]
        .as_array()
        .ok_or("stats without sessions")?
        .iter()
        .map(|s| {
            (
                s["id"].as_str().unwrap_or_default().to_owned(),
                s["requests"].as_u64().unwrap_or_default(),
            )
        })
        .collect())
}

/// The tool-result text of the sidecar's answer to the origin's retrieve request, if the
/// session's calls hold one.
fn retrieve_answer(calls: &[Recorded]) -> Option<String> {
    calls.iter().find_map(|c| {
        let body: Value = serde_json::from_slice(&c.body).ok()?;
        let last = body["messages"].as_array()?.last()?.clone();
        let part = last["content"].as_array()?.first()?.clone();
        (part["type"] == "tool_result" && part["tool_use_id"] == "toolu_retrieve")
            .then(|| part["content"].as_str().unwrap_or_default().to_owned())
    })
}
/// The file path a retrieve answer points the model at, when the original was exported.
fn exported_path(answer: &str) -> Option<PathBuf> {
    let rest = answer.split("path=\"").nth(1)?;
    Some(PathBuf::from(rest.split('"').next()?))
}

#[test]
#[ignore = "native: starts the pinned sidecar"]
fn two_sessions_with_identical_prefixes_do_not_share_state() {
    let (sidecar, origin) = proof();
    let (a, b) = (new_session(), new_session());
    // Same system prompt, same first ten messages, a tools array; only the tool result's
    // marker differs. Its newest message is the large result, above the CCR threshold, so
    // the sidecar stores it and puts a placeholder on the wire.
    thread::scope(|scope| {
        let first = scope.spawn(|| {
            let mut history = open_session(sidecar.port, &origin, &a, "A", "MARKER_A")?;
            run_turns(sidecar.port, &origin, &a, &mut history, "A", 1..TURNS)
        });
        let second = scope.spawn(|| {
            let mut history = open_session(sidecar.port, &origin, &b, "B", "MARKER_B")?;
            run_turns(sidecar.port, &origin, &b, &mut history, "B", 1..TURNS)
        });
        first.join().expect("session A thread").expect("session A");
        second.join().expect("session B thread").expect("session B");
    });
    let calls = model_calls_by_session(&origin, &[&a, &b]).expect("attribute every model call");
    let (calls_a, calls_b) = (&calls[&a], &calls[&b]);
    // Not vacuous: each session reached the origin with every request ...
    assert!(calls_a.len() >= TURNS && calls_b.len() >= TURNS);
    // ... and CCR really fired: every body of a session carries the stored-content
    // placeholder for its own marker result, and the original (whose closing marker would
    // make a second occurrence) is not on the wire.
    for (calls, own, other) in [
        (calls_a, "MARKER_A", "MARKER_B"),
        (calls_b, "MARKER_B", "MARKER_A"),
    ] {
        assert_eq!(
            count_in(calls, "[acp-stored"),
            calls.len(),
            "{own}: placeholder missing"
        );
        for call in calls {
            let body = String::from_utf8_lossy(&call.body);
            assert_eq!(
                body.matches(own).count(),
                1,
                "{own}: the original is on the wire"
            );
            // Isolation: nothing of one session appears in a body sent on behalf of the other.
            assert_eq!(
                body.matches(other).count(),
                0,
                "{other} leaked into a body of {own}"
            );
        }
    }
    // The retrieve path ran for both sessions.
    for (calls, label) in [(calls_a, "A"), (calls_b, "B")] {
        let answer = retrieve_answer(calls)
            .unwrap_or_else(|| panic!("session {label} never got a retrieve answer"));
        assert!(answer.starts_with("[acp-retrieved #"), "{label}: {answer}");
    }
    // The sidecar's own accounting: two distinct sessions, each with all its requests, and
    // the ids are the ones the clients sent.
    let seen = stats(sidecar.port).expect("stats");
    let mut ids: Vec<&str> = seen.iter().map(|(id, _)| id.as_str()).collect();
    ids.sort_unstable();
    let mut sent = [a.as_str(), b.as_str()];
    sent.sort_unstable();
    assert_eq!(
        ids, sent,
        "stats must list exactly the two sessions: {seen:?}"
    );
    for (id, requests) in &seen {
        assert_eq!(*requests, TURNS as u64, "session {id} requests: {seen:?}");
    }
}

#[test]
#[ignore = "native: starts the pinned sidecar"]
fn retrieved_exports_of_two_sessions_do_not_collide() {
    // Two sessions with identical prefixes store the same-numbered placeholder. Each asks
    // for its stored original; the sidecar exports a large original to a file and points
    // the model at it. The two pointers must name different files, and each file must
    // keep its own session's original.
    let (sidecar, origin) = proof();
    let (a, b) = (new_session(), new_session());
    let mut exports = Vec::new();
    for (session, label, marker) in [(&a, "A", "MARKER_A"), (&b, "B", "MARKER_B")] {
        let mut history =
            open_session(sidecar.port, &origin, session, label, marker).expect("open the session");
        run_turns(
            sidecar.port,
            &origin,
            session,
            &mut history,
            label,
            1..RETRIEVE_TURN + 1,
        )
        .expect("turns up to the retrieve");
        let calls = model_calls_by_session(&origin, &[&a, &b]).expect("attribute");
        let answer = retrieve_answer(&calls[session.as_str()]).expect("a retrieve answer");
        let path = exported_path(&answer).unwrap_or_else(|| panic!("no exported path: {answer}"));
        let content = fs::read_to_string(&path).expect("read the exported original");
        assert!(
            content.contains(marker),
            "{label}: the export does not hold {marker}'s original"
        );
        exports.push(path);
    }
    assert_ne!(
        exports[0], exports[1],
        "both sessions were pointed at the same file {:?}",
        exports[0]
    );
    // B's retrieve must not have changed what A's pointer resolves to.
    let after = fs::read_to_string(&exports[0]).expect("A's export, after B's retrieve");
    assert!(
        after.contains("MARKER_A") && !after.contains("MARKER_B"),
        "A's export now holds B's original"
    );
}

#[test]
#[ignore = "native: starts the pinned sidecar"]
fn resumed_session_keeps_its_own_identity() {
    let (sidecar, origin) = proof();
    let session = new_session();
    // Request 1 opens the session, requests 2-5 are turns 1-4.
    let mut history =
        open_session(sidecar.port, &origin, &session, "A", "MARKER_A").expect("opening request");
    run_turns(sidecar.port, &origin, &session, &mut history, "A", 1..5).expect("first turns");
    // The client goes away and comes back with the same session id: a fresh client replays
    // the transcript it saved, which is the conversation as it stood after turn four.
    let mut resumed = history.clone();
    drop(history);
    run_turns(sidecar.port, &origin, &session, &mut resumed, "A", 5..10).expect("last five turns");
    let calls = model_calls_by_session(&origin, &[&session]).expect("attribute every model call");
    assert!(calls[&session].len() >= 10);
    let seen = stats(sidecar.port).expect("stats");
    assert_eq!(
        seen.len(),
        1,
        "a resumed session must stay one session: {seen:?}"
    );
    assert_eq!(
        seen[0].0, session,
        "the one session is the one the client sent"
    );
    assert_eq!(seen[0].1, 10, "requests of the one session: {seen:?}");
}

/// A streamed probe in flight: the open connection and everything read from it so far, so
/// a test can act after the first SSE byte and read the rest afterwards.
struct Streaming {
    stream: TcpStream,
    bytes: Vec<u8>,
}
impl Streaming {
    fn open(port: u16, probe: &Probe) -> ProofResult<Self> {
        Ok(Self {
            stream: send_request(port, "POST", &probe.path, &probe.headers, &probe.body)?,
            bytes: Vec::new(),
        })
    }
    /// Reads until `needle` has arrived (raw bytes, chunk framing included).
    fn read_until(&mut self, needle: &str, within: Duration) -> ProofResult<()> {
        let deadline = Instant::now() + within;
        safe(
            self.stream
                .set_read_timeout(Some(Duration::from_millis(50))),
            "bound stream read",
        )?;
        let mut buffer = [0_u8; 4096];
        while !String::from_utf8_lossy(&self.bytes).contains(needle) {
            check(Instant::now() < deadline, "stream byte deadline")?;
            match self.stream.read(&mut buffer) {
                Ok(0) => return Err(format!("stream ended before {needle:?}")),
                Ok(n) => self.bytes.extend_from_slice(&buffer[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(e) => return Err(format!("stream error before {needle:?}: {e}")),
            }
        }
        Ok(())
    }
    /// Reads until the connection closes. The flag is true for an orderly close, false when
    /// the peer vanished (reset or aborted), which is what a killed sidecar looks like.
    fn finish(mut self, within: Duration) -> ProofResult<(Vec<u8>, bool)> {
        let deadline = Instant::now() + within;
        safe(
            self.stream
                .set_read_timeout(Some(Duration::from_millis(50))),
            "bound stream read",
        )?;
        let mut buffer = [0_u8; 4096];
        loop {
            check(Instant::now() < deadline, "stream end deadline")?;
            match self.stream.read(&mut buffer) {
                Ok(0) => return Ok((self.bytes, true)),
                Ok(n) => self.bytes.extend_from_slice(&buffer[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => return Ok((self.bytes, false)),
            }
        }
    }
}
/// The tagged streaming Messages requests the origin holds for `probe`: a replay of the
/// client's turn shows up here as a second copy. Rewrite calls the sidecar makes on its
/// own are not streamed and are not replays of the turn, so they are not counted.
fn copies_of(origin: &Origin, probe: &Probe) -> usize {
    origin
        .all_requests()
        .iter()
        .filter(|r| is_probe(r, &probe.tag))
        .count()
}

#[test]
#[ignore = "native: starts the pinned sidecar"]
fn drain_lets_a_running_stream_finish() {
    let sidecar = Running::start().expect("start the pinned sidecar");
    // The origin holds message_stop back for three seconds: the stream is still running
    // when the drain is requested.
    let origin = Origin::start(&sidecar, 3000).expect("start the synthetic origin");
    let session = new_session();
    let probe = probe(&origin, &session, &[], "Scrivi una frase.", None);
    let mut streaming = Streaming::open(sidecar.port, &probe).expect("open the stream");
    streaming
        .read_until("message_start", Duration::from_secs(30))
        .expect("first SSE byte");
    let calls_at_first_byte = origin.requests().len();
    // The turn is the only model call so far: nothing was rewritten or summarised first.
    assert_eq!(calls_at_first_byte, 1);
    let pid = sidecar
        .supervisor
        .status()
        .and_then(|s| s.pid)
        .expect("the owner holds the sidecar pid");
    sidecar
        .supervisor
        .request(Command::Drain)
        .expect("request the drain");
    // The client keeps reading the stream while the test watches the owner.
    let client = thread::spawn(move || {
        let read = streaming.finish(Duration::from_secs(60));
        (read, Instant::now())
    });
    let mut samples: Vec<(Instant, Phase, Option<u64>, Option<u32>)> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    let stopped_at = loop {
        assert!(Instant::now() < deadline, "the drain never reached Stopped");
        if let Some(s) = sidecar.supervisor.status() {
            samples.push((Instant::now(), s.phase, s.in_flight, s.pid));
            if s.phase == Phase::Stopped {
                break Instant::now();
            }
            assert!(
                !matches!(s.phase, Phase::Failed | Phase::Retired),
                "the drain ended in {:?} / {:?}",
                s.phase,
                s.reason
            );
        }
        thread::sleep(Duration::from_millis(5));
    };
    let (read, finished_at) = client.join().expect("client thread");
    let (bytes, orderly) = read.expect("the client reads the stream to its end");
    // The client got the whole stream, ending in message_stop, on an orderly close.
    let response = parse_response(&bytes).expect("a complete HTTP response");
    let text = String::from_utf8_lossy(&response.body);
    assert_eq!(response.status, 200);
    assert!(orderly, "the sidecar must close the stream in order");
    assert!(text.contains(REPLY), "stream body: {text}");
    assert!(
        text.trim_end()
            .ends_with(r#"data: {"type":"message_stop"}"#),
        "the stream must end in message_stop: {text}"
    );
    // The worker glue, end to end: Draining while the stream runs, on the same process,
    // with n > 0 requests in flight, then 0, and only then Stopped.
    let busy = samples
        .iter()
        .position(|(at, phase, n, _)| {
            *phase == Phase::Draining && *at < finished_at && n.is_some_and(|n| n > 0)
        })
        .unwrap_or_else(|| {
            panic!("never saw Draining with in_flight > 0 while the stream ran: {samples:?}")
        });
    assert!(
        samples[..=busy]
            .iter()
            .filter(|(_, phase, ..)| *phase == Phase::Draining)
            .all(|(_, _, _, p)| *p == Some(pid)),
        "a drain keeps the same live process: {samples:?}"
    );
    assert!(
        samples[busy..].iter().any(|(_, _, n, _)| *n == Some(0)),
        "in_flight never dropped to 0 after the stream ended: {samples:?}"
    );
    assert!(
        finished_at <= stopped_at,
        "Stopped before the stream finished"
    );
    let end = sidecar.supervisor.status().expect("final status");
    assert_eq!(end.phase, Phase::Stopped);
    assert!(!end.restart_used, "a drained sidecar is not restarted");
    assert_eq!(
        end.reason,
        Reason::None,
        "the drain must end by idling, not by timeout"
    );
    // Exactly one copy of the client's turn reached the origin, and nothing else was
    // sent to a model after the first byte.
    assert_eq!(copies_of(&origin, &probe), 1);
    assert_eq!(origin.requests().len(), calls_at_first_byte);
    // A stop on the stopped owner is harmless.
    sidecar
        .supervisor
        .request(Command::Stop)
        .expect("stop request on a stopped owner");
    thread::sleep(Duration::from_millis(500));
    let after = sidecar
        .supervisor
        .status()
        .expect("status after the extra stop");
    assert_eq!(after.phase, Phase::Stopped);
    assert_eq!(after.pid, None);
}

#[test]
#[ignore = "native: starts the pinned sidecar"]
fn killed_sidecar_never_replays_the_interrupted_request() {
    let sidecar = Running::start().expect("start the pinned sidecar");
    let origin = Origin::start(&sidecar, 3000).expect("start the synthetic origin");
    let session = new_session();
    let probe = probe(&origin, &session, &[], "Scrivi una frase.", None);
    let mut streaming = Streaming::open(sidecar.port, &probe).expect("open the stream");
    streaming
        .read_until("message_start", Duration::from_secs(30))
        .expect("first SSE byte");
    let calls_at_first_byte = origin.requests().len();
    let first = sidecar.supervisor.status().expect("status");
    let pid = first.pid.expect("the owner holds the sidecar pid");
    assert!(!first.restart_used);
    // Crash the sidecar process mid-stream (that process only, no child tree).
    let killed = std::process::Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run taskkill");
    assert!(killed.success(), "could not terminate the sidecar process");
    // The client sees the stream cut off, never a finished one.
    let (bytes, _) = streaming
        .finish(Duration::from_secs(30))
        .expect("the client's connection ends");
    assert!(
        !String::from_utf8_lossy(&bytes).contains("message_stop"),
        "the stream must have been interrupted"
    );
    // The owner restarts it once, on the same port, as a new process.
    let restarted = Running::wait_status(
        &sidecar.supervisor,
        Duration::from_secs(60),
        Phase::Ready,
        |s| s.healthy && s.restart_used && s.pid.is_some_and(|p| p != pid),
    )
    .expect("the owner restarts the sidecar");
    let health = sidecar.health().expect("health on the original port");
    assert_eq!(health["pid"].as_u64(), restarted.pid.map(u64::from));
    // Longer than the origin's own delay: a replayed request would have arrived by now.
    thread::sleep(Duration::from_secs(5));
    assert_eq!(
        copies_of(&origin, &probe),
        1,
        "the interrupted request was replayed"
    );
    assert_eq!(
        origin.requests().len(),
        calls_at_first_byte,
        "a model call was made after the crash"
    );
    // The owner settled on the one restart: same process as after it, no second restart.
    let end = sidecar.supervisor.status().expect("final status");
    assert!(end.restart_used && end.phase == Phase::Ready);
    assert_eq!(
        end.pid, restarted.pid,
        "the restarted process was replaced again"
    );
    assert_eq!(
        Running::wait_status(
            &sidecar.supervisor,
            Duration::from_secs(1),
            Phase::Ready,
            |s| s.healthy && s.pid == restarted.pid
        )
        .map(|s| s.reason),
        Ok(Reason::None)
    );
}
