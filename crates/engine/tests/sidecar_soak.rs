#![cfg(windows)]
#![allow(dead_code)] // The shared harness serves several proofs; each uses a subset.
//! SC16: five concurrent clients through the pinned sidecar for a fixed time, with a sixth
//! client that, every 250 ms, measures a `CONNECT` plus a local echo and `GET /__bili/health`.
//! The only upstream is the synthetic local origin (fixtures/origin.mjs, started without
//! recording), so no paid call can happen. Each of the five clients keeps a session whose
//! history grows to 150_000 estimated tokens (four bytes per token) and sends a tool result
//! above the 4_000-token CCR threshold on every third turn; at the cap it opens a new session.
//!
//! The origin holds each stream open for `SHINATAMA_SOAK_STREAM_MS` (default 3000) before
//! it ends, the way a model keeps a turn running; 0 makes the five clients send back to
//! back, a saturation stress rather than the SC16 load.
//! The run length is `SHINATAMA_SOAK_SECONDS` (default 3600). The measures are written to
//! `target/sc16-soak.json`. The proof reads latency from the outside only: the sidecar's
//! internal event-loop delay (`monitorEventLoopDelay`) is not observable from here.
//!
//!   SHINATAMA_SOAK_SECONDS=120 cargo test -p shinatama-engine --test sidecar_soak -- --ignored
#[path = "support/sidecar_harness.rs"]
mod sidecar_harness;
use serde_json::{Value, json};
use shinatama_engine::sidecar_admin::{AdminTarget, in_flight};
use sidecar_harness::*;
use std::{
    fs,
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    num::NonZeroU16,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

const CLIENTS: usize = 5;
/// Estimated tokens at which a client's session ends and a new one starts.
const TOKEN_CAP: usize = 150_000;
const BYTES_PER_TOKEN: usize = 4;
/// A tool result goes into every third turn.
const TOOL_EVERY: usize = 3;
/// The per-stream output cap the product's owner enforces on the sidecar (64 MiB).
const OUTPUT_LIMIT: u64 = 64 * 1024 * 1024;
const PERIOD: Duration = Duration::from_millis(250);
const DEFAULT_SECONDS: u64 = 3600;
/// What a `CONNECT` the sidecar did not answer with 200 is called.
const CONNECT_REFUSED: &str = "CONNECT refused";
const PING: &[u8; 12] = b"latency-ping";

// The origin handle must be shareable so the clients can run from threads.
const _: fn() = || {
    fn sync<T: Sync>() {}
    sync::<Origin>();
};

/// How long the origin keeps each reply stream open, as a model's generation would.
const DEFAULT_STREAM_MS: u64 = 3000;

fn read_stream_ms() -> u64 {
    std::env::var("SHINATAMA_SOAK_STREAM_MS")
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(DEFAULT_STREAM_MS)
}
fn soak_seconds() -> u64 {
    std::env::var("SHINATAMA_SOAK_SECONDS")
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .filter(|seconds| *seconds > 0)
        .unwrap_or(DEFAULT_SECONDS)
}
fn millis(since: Instant) -> f64 {
    (since.elapsed().as_secs_f64() * 10_000.0).round() / 10.0
}

/// Why a request or probe failed. A refusal (the sidecar did not accept the connection, or
/// answered a `CONNECT` with something other than 200) is what SC16 counts; anything else is
/// a failure of the run. The kind is decided once, where the failure is made.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Failure {
    Refused(String),
    Other(String),
}
impl Failure {
    /// Classifies an error code of the shared harness (whose errors are plain strings).
    fn from_harness(code: String) -> Self {
        if code == CONNECT_FAILED {
            Self::Refused(code)
        } else {
            Self::Other(code)
        }
    }
    /// The same failure with context in front of its description; the kind is kept.
    fn context(self, context: &str) -> Self {
        match self {
            Self::Refused(what) => Self::Refused(format!("{context}: {what}")),
            Self::Other(what) => Self::Other(format!("{context}: {what}")),
        }
    }
}

/// Failures described in the assertion message at most.
const KEPT_ERRORS: usize = 5;

/// What one thread observed. `refused` counts refusals; `failures` counts every other
/// failure, of which `errors` keeps the first few described.
#[derive(Default)]
struct Observed {
    requests: u64,
    refused: u64,
    failures: u64,
    sessions: usize,
    errors: Vec<String>,
    longest_hold_ms: f64,
    connect_ms: Vec<f64>,
    health_ms: Vec<f64>,
}
impl Observed {
    fn failed(&mut self, failure: Failure) {
        match failure {
            Failure::Refused(_) => self.refused += 1,
            Failure::Other(what) => {
                self.failures += 1;
                if self.errors.len() < KEPT_ERRORS {
                    self.errors.push(what);
                }
            }
        }
    }
}

/// A tool result of `tokens` estimated tokens, different for every client and turn.
fn tool_text(client: usize, turn: usize, tokens: usize) -> String {
    let mut seed = 0x9e37_79b9_u64 ^ ((client as u64) << 40) ^ ((turn as u64) << 8);
    let mut text = String::with_capacity(tokens * BYTES_PER_TOKEN + 16);
    while text.len() < tokens * BYTES_PER_TOKEN {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        text.push_str(&format!("{:x} ", seed >> 24));
    }
    text
}

/// One client's conversation: the history so far and its size.
struct Chat {
    client: usize,
    session: String,
    history: Vec<Msg>,
    bytes: usize,
    turn: usize,
    sessions: usize,
}
impl Chat {
    fn new(client: usize) -> Self {
        Self {
            client,
            session: new_session(),
            history: Vec::new(),
            bytes: 0,
            turn: 0,
            sessions: 1,
        }
    }
    /// The next request: the messages to send, and the ones the conversation keeps once
    /// the reply is in (everything sent after the history, then the assistant's answer).
    fn next(&mut self) -> (Vec<Value>, Vec<Msg>) {
        self.turn += 1;
        let turn = self.turn;
        let mut added = Vec::new();
        let newest = if turn.is_multiple_of(TOOL_EVERY) {
            let id = format!("toolu_{}_{turn}", self.client);
            added.push(Msg::user_text(&format!("Turno {turn}: leggi il file.")));
            added.push(Msg::assistant_tool_use(
                &id,
                "Read",
                json!({ "file_path": format!(r"C:\synthetic\file-{turn}.txt") }),
            ));
            // 5_000 to 6_800 tokens, always above the 4_000-token CCR threshold.
            let tokens = 5_000 + (turn % 7) * 300;
            Msg::tool_result(&id, &tool_text(self.client, turn, tokens))
        } else {
            Msg::user_text(&format!("Turno {turn}: continua il lavoro."))
        };
        let mut messages: Vec<Value> = self.history.iter().map(Msg::wire).collect();
        messages.extend(added.iter().map(Msg::wire));
        messages.push(newest.wire());
        added.push(newest);
        added.push(Msg::assistant_text(REPLY));
        (messages, added)
    }
    fn keep(&mut self, added: Vec<Msg>) {
        self.bytes += added
            .iter()
            .map(|m| m.content.to_string().len())
            .sum::<usize>();
        self.history.extend(added);
        if self.bytes >= TOKEN_CAP * BYTES_PER_TOKEN {
            self.session = new_session();
            self.history.clear();
            self.bytes = 0;
            self.turn = 0;
            self.sessions += 1;
        }
    }
}

/// One client: turn after turn until `deadline`, each one resent with the whole history.
fn client(
    port: u16,
    origin: &Origin,
    index: usize,
    stream_ms: u64,
    begin: Instant,
    deadline: Instant,
) -> Observed {
    let mut seen = Observed::default();
    let mut chat = Chat::new(index);
    let tools = read_tool();
    while Instant::now() < deadline {
        let (messages, added) = chat.next();
        let probe = probe_messages(origin, &chat.session, messages, Some(&tools));
        let started = Instant::now();
        let outcome =
            request(port, "POST", &probe.path, &probe.headers, &probe.body).and_then(|response| {
                check(response.status == 200, "sidecar did not answer with 200")?;
                check(
                    String::from_utf8_lossy(&response.body).contains(REPLY),
                    "reply did not come from the synthetic origin",
                )
            });
        match outcome {
            Ok(()) => {
                seen.requests += 1;
                let hold = millis(started);
                if hold >= (stream_ms + 750) as f64 {
                    eprintln!(
                        "SC16 slow request {hold} ms: client {index} turn {} (~{} tokens) at +{:.1}s",
                        chat.turn,
                        chat.bytes / BYTES_PER_TOKEN,
                        begin.elapsed().as_secs_f64()
                    );
                }
                seen.longest_hold_ms = seen.longest_hold_ms.max(hold);
                chat.keep(added);
            }
            Err(what) => {
                seen.failed(Failure::from_harness(what).context(&format!("client {index}")));
                // The turn is not lost for the next attempt: only the history is kept as is.
                chat.turn -= 1;
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    seen.sessions = chat.sessions;
    seen
}

/// A loopback echo target for the tunnel: reads the twelve ping bytes and returns them.
fn echo(listener: TcpListener, stop: &AtomicBool) {
    listener.set_nonblocking(true).expect("bound echo listener");
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut socket, _)) => {
                thread::spawn(move || {
                    let _ = socket.set_nonblocking(false);
                    let _ = socket.set_read_timeout(Some(Duration::from_secs(5)));
                    let _ = socket.set_write_timeout(Some(Duration::from_secs(5)));
                    let mut bytes = [0; 12];
                    if socket.read_exact(&mut bytes).is_ok() {
                        let _ = socket.write_all(&bytes);
                    }
                });
            }
            Err(_) => thread::sleep(Duration::from_millis(5)),
        }
    }
}
/// The sidecar's answer to a `CONNECT`: only a 200 opens the tunnel; anything else is a refusal.
fn tunnel_answer(response: &[u8]) -> Result<(), Failure> {
    if response.starts_with(b"HTTP/1.1 200") {
        Ok(())
    } else {
        Err(Failure::Refused(CONNECT_REFUSED.to_owned()))
    }
}
/// `CONNECT` to the echo target through the sidecar, send the ping, read it back. The
/// time is the whole exchange: TCP connect, tunnel set up, round trip.
fn connect_and_echo(port: u16, echo_port: u16) -> Result<f64, Failure> {
    let other = Failure::Other;
    let started = Instant::now();
    let mut stream =
        TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), Duration::from_secs(2))
            .map_err(|_| Failure::Refused(CONNECT_FAILED.to_owned()))?;
    safe(
        stream.set_read_timeout(Some(Duration::from_secs(5))),
        "bound CONNECT read",
    )
    .map_err(other)?;
    safe(
        stream.set_write_timeout(Some(Duration::from_secs(5))),
        "bound CONNECT write",
    )
    .map_err(other)?;
    safe(
        write!(
            stream,
            "CONNECT 127.0.0.1:{echo_port} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"
        ),
        "send CONNECT",
    )
    .map_err(other)?;
    let mut response = Vec::new();
    let mut byte = [0];
    while !response.ends_with(b"\r\n\r\n") && response.len() < 1024 {
        safe(stream.read_exact(&mut byte), "read CONNECT response").map_err(other)?;
        response.push(byte[0]);
    }
    tunnel_answer(&response)?;
    safe(stream.write_all(PING), "write tunnel ping").map_err(other)?;
    let mut echoed = [0; 12];
    safe(stream.read_exact(&mut echoed), "read tunnel echo").map_err(other)?;
    check(&echoed == PING, "tunnel echo mismatch").map_err(other)?;
    Ok(millis(started))
}
/// The sixth client: every 250 ms a CONNECT plus echo and a health read, until `deadline`.
fn measuring(port: u16, echo_port: u16, begin: Instant, deadline: Instant) -> Observed {
    let mut seen = Observed::default();
    while Instant::now() < deadline {
        let round = Instant::now();
        match connect_and_echo(port, echo_port) {
            Ok(ms) => {
                if ms >= 750.0 {
                    eprintln!(
                        "SC16 slow CONNECT+echo {ms} ms at +{:.1}s",
                        begin.elapsed().as_secs_f64()
                    );
                }
                seen.connect_ms.push(ms)
            }
            Err(failure) => seen.failed(failure.context("CONNECT probe")),
        }
        let health = Instant::now();
        match request(port, "GET", "/__bili/health", &[], b"")
            .and_then(|response| check(response.status == 200, "health did not answer 200"))
        {
            Ok(()) => seen.health_ms.push(millis(health)),
            Err(what) => seen.failed(Failure::from_harness(what).context("health probe")),
        }
        if let Some(rest) = PERIOD.checked_sub(round.elapsed()) {
            thread::sleep(rest);
        }
    }
    seen
}

/// Nearest-rank percentile of a sample, 0 when empty.
fn percentile(sorted: &[f64], percent: usize) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (percent * sorted.len())
        .div_ceil(100)
        .clamp(1, sorted.len());
    sorted[rank - 1]
}

struct SoakResult {
    seconds: u64,
    requests: u64,
    refused_connections: u64,
    connect_ms: Vec<f64>,
    health_ms_max: f64,
    longest_hold_ms: f64,
    stdout_bytes: u64,
    stderr_bytes: u64,
    sessions: usize,
    failures: u64,
    errors: Vec<String>,
}
impl SoakResult {
    fn connect_max(&self) -> f64 {
        self.connect_ms.iter().copied().fold(0.0, f64::max)
    }
    fn json(&self) -> Value {
        let mut sorted = self.connect_ms.clone();
        sorted.sort_by(f64::total_cmp);
        json!({
            "seconds": self.seconds,
            "clients": CLIENTS,
            "requests": self.requests,
            "refusedConnections": self.refused_connections,
            "connectMsP50": percentile(&sorted, 50),
            "connectMsP95": percentile(&sorted, 95),
            "connectMsMax": self.connect_max(),
            "healthMsMax": self.health_ms_max,
            "longestHoldMs": self.longest_hold_ms,
            "stdoutBytes": self.stdout_bytes,
            "stderrBytes": self.stderr_bytes,
        })
    }
}

fn soak(sidecar: &Running, origin: &Origin, seconds: u64, stream_ms: u64) -> SoakResult {
    let echo_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind echo target");
    let echo_port = echo_listener.local_addr().unwrap().port();
    let port = sidecar.port;
    let stop = AtomicBool::new(false);
    let begin = Instant::now();
    let deadline = begin + Duration::from_secs(seconds);
    let (clients, measure) = thread::scope(|scope| {
        let echo_worker = scope.spawn(|| echo(echo_listener, &stop));
        let workers: Vec<_> = (0..CLIENTS)
            .map(|index| {
                scope.spawn(move || client(port, origin, index, stream_ms, begin, deadline))
            })
            .collect();
        let measure = scope.spawn(move || measuring(port, echo_port, begin, deadline));
        let clients: Vec<Observed> = workers
            .into_iter()
            .map(|worker| worker.join().expect("client thread"))
            .collect();
        let measure = measure.join().expect("measuring thread");
        stop.store(true, Ordering::Release);
        echo_worker.join().expect("echo thread");
        (clients, measure)
    });
    let snapshot = sidecar.supervisor.status().expect("supervisor status");
    let mut errors: Vec<String> = Vec::new();
    for seen in clients.iter().chain([&measure]) {
        errors.extend(seen.errors.iter().cloned());
    }
    let failures: u64 = clients
        .iter()
        .chain([&measure])
        .map(|seen| seen.failures)
        .sum();
    SoakResult {
        seconds,
        requests: clients.iter().map(|c| c.requests).sum(),
        refused_connections: clients
            .iter()
            .chain([&measure])
            .map(|seen| seen.refused)
            .sum(),
        connect_ms: measure.connect_ms,
        health_ms_max: measure.health_ms.iter().copied().fold(0.0, f64::max),
        longest_hold_ms: clients
            .iter()
            .map(|c| c.longest_hold_ms)
            .fold(0.0, f64::max),
        stdout_bytes: snapshot.stdout_bytes,
        stderr_bytes: snapshot.stderr_bytes,
        sessions: clients.iter().map(|c| c.sessions).sum(),
        failures,
        errors,
    }
}

fn result_path() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace().join("target"));
    target.join("sc16-soak.json")
}

#[test]
#[ignore = "native soak: one hour"]
fn five_instances_for_one_hour_refuse_no_connection() {
    let seconds = soak_seconds();
    let stream_ms = read_stream_ms();
    let sidecar = Running::start_with_output_limit(OUTPUT_LIMIT).expect("start the pinned sidecar");
    let origin = Origin::start_unrecorded(&sidecar, stream_ms).expect("start the synthetic origin");
    assert!(
        sidecar.health().expect("health")["pid"].is_u64(),
        "the sidecar must be the pinned CLI"
    );
    let result = soak(&sidecar, &origin, seconds, stream_ms);
    let report = result.json();
    let path = result_path();
    fs::create_dir_all(path.parent().unwrap()).expect("create the result directory");
    fs::write(&path, serde_json::to_vec_pretty(&report).unwrap()).expect("write the result");
    eprintln!("SC16 soak: {report} (sessions opened: {})", result.sessions);

    assert_eq!(result.refused_connections, 0);
    assert!(result.connect_max() < 750.0, "SC16 local CONNECT bound");
    assert!(result.health_ms_max < 750.0, "SC16 local health bound");
    assert!(result.stdout_bytes < 64 * 1024 * 1024 && result.stderr_bytes < 64 * 1024 * 1024);
    // Beyond the brief: the proof is only meaningful if the load really ran and every
    // request that was not a refused connection also succeeded.
    assert!(result.requests > 0, "no request completed");
    assert!(!result.connect_ms.is_empty(), "no CONNECT sample");
    assert_eq!(
        result.failures,
        0,
        "{} failures, the first {}: {:?}",
        result.failures,
        result.errors.len(),
        result.errors
    );

    // After the clients stop the sidecar holds no request.
    let pid = sidecar
        .supervisor
        .status()
        .and_then(|s| s.pid)
        .expect("sidecar pid");
    let target = AdminTarget {
        port: NonZeroU16::new(sidecar.port).unwrap(),
        pid,
        timeout: Duration::from_secs(2),
    };
    let until = Instant::now() + Duration::from_secs(10);
    let mut held = in_flight(&target);
    while held != Some(0) && Instant::now() < until {
        thread::sleep(Duration::from_millis(200));
        held = in_flight(&target);
    }
    assert_eq!(held, Some(0), "the sidecar still holds a request");
}

// Pure checks of how a failure is classified: no sidecar, no origin.
#[test]
fn a_refused_connection_is_counted_whatever_context_wraps_it() {
    let refused = Failure::from_harness(CONNECT_FAILED.to_owned());
    assert_eq!(refused, Failure::Refused(CONNECT_FAILED.to_owned()));
    // The callers wrap the description before recording it: the kind must survive.
    let mut seen = Observed::default();
    seen.failed(refused.clone().context("client 3"));
    seen.failed(refused.context("CONNECT probe"));
    assert_eq!((seen.refused, seen.failures), (2, 0));
    assert!(seen.errors.is_empty());
}
#[test]
fn a_connect_not_answered_with_200_is_a_refusal() {
    assert_eq!(
        tunnel_answer(b"HTTP/1.1 200 Connection Established\r\n\r\n"),
        Ok(())
    );
    for answer in [
        &b"HTTP/1.1 403 Forbidden\r\n\r\n"[..],
        b"HTTP/1.1 504 Gateway Timeout\r\n\r\n",
        b"",
    ] {
        let refused = tunnel_answer(answer).unwrap_err();
        assert_eq!(refused, Failure::Refused(CONNECT_REFUSED.to_owned()));
        let mut seen = Observed::default();
        seen.failed(refused.context("CONNECT probe"));
        assert_eq!((seen.refused, seen.failures), (1, 0));
    }
}
#[test]
fn other_failures_are_counted_in_full_and_described_up_to_a_cap() {
    let mut seen = Observed::default();
    for n in 0..KEPT_ERRORS + 3 {
        seen.failed(Failure::from_harness(format!("read response {n}")).context("client 0"));
    }
    assert_eq!(seen.refused, 0);
    assert_eq!(seen.failures, (KEPT_ERRORS + 3) as u64);
    assert_eq!(seen.errors.len(), KEPT_ERRORS);
    assert!(seen.errors[0].starts_with("client 0: read response 0"));
}
