#![cfg(windows)]
//! C32 native proof: the real `shinatama mcp` process over stdio. Every stdout line must be a
//! JSON-RPC message; a wrong token is refused; startup to the initialize answer is measured.
use shinatama_messaging::auth::{new_token, token_hash};
use shinatama_state::location::{LocationRequest, StorageLocation};
use shinatama_state::messages::MessageStore;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
}

fn session(home: &Path, cwd: &Path, instance: &str, token: &str, requests: &[String]) -> (Vec<serde_json::Value>, Duration, String) {
    let started = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_shinatama"))
        .arg("mcp")
        .env("SHINATAMA_HOME", home)
        .env("SHINATAMA_INSTANCE_ID", instance)
        .env("SHINATAMA_INSTANCE_TOKEN", token)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut responses = Vec::new();
    let mut first = None;
    for request in requests {
        writeln!(stdin, "{request}").unwrap();
        stdin.flush().unwrap();
        let expects_answer = serde_json::from_str::<serde_json::Value>(request).map(|v| v.get("id").is_some()).unwrap_or(true);
        if expects_answer {
            let mut line = String::new();
            stdout.read_line(&mut line).unwrap();
            first.get_or_insert_with(|| started.elapsed());
            responses.push(serde_json::from_str::<serde_json::Value>(line.trim()).unwrap_or_else(|_| panic!("stdout is not JSON-RPC: {line:?}")));
        }
    }
    drop(stdin);
    let mut rest = String::new();
    std::io::Read::read_to_string(&mut stdout, &mut rest).unwrap();
    assert!(rest.trim().is_empty(), "nothing else on stdout: {rest:?}");
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "exit {:?}", output.status);
    (responses, first.unwrap_or_default(), String::from_utf8_lossy(&output.stderr).into_owned())
}

#[test]
fn real_mcp_process_speaks_only_json_rpc_and_refuses_impostors() {
    let parent = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    let base = parent.join(format!("shinatama-mcp-proof-{}", now()));
    let (home, cwd) = (base.join("home"), base.join("cwd"));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&cwd).unwrap();
    let location = StorageLocation::resolve(LocationRequest { local_data_directory: parent.clone(), home_override: Some(home.clone()), project_roots: vec![cwd.clone()], synchronized_roots: vec![] }).unwrap();
    location.prepare_private_directory().unwrap();
    let store = MessageStore::open(&location).unwrap();
    let (ta, tb) = (new_token(), new_token());
    store.register_instance("i_a", "claude_code", "C:/p", Some("t_a"), None, &token_hash(&ta), "[\"mcp_pull\"]", now()).unwrap();
    store.register_instance("i_b", "codex", "C:/p", Some("t_b"), None, &token_hash(&tb), "[\"mcp_pull\"]", now()).unwrap();
    let call = |id: u32, name: &str, args: serde_json::Value| serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}}).to_string();
    let requests = vec![
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"proof","version":"1"}}}"#.to_owned(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_owned(),
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#.to_owned(),
        "{this is not json".to_owned(),
        call(3, "shinatama_send", serde_json::json!({"instance":"i_b","kind":"note","body":"hello from a"})),
    ];
    let (responses, startup, stderr) = session(&home, &cwd, "i_a", &ta, &requests);
    assert_eq!(responses[0]["result"]["serverInfo"]["name"], "shinatama");
    assert_eq!(responses[1]["result"]["tools"].as_array().unwrap().len(), 4);
    assert_eq!(responses[2]["error"]["code"], -32700);
    assert_eq!(responses[3]["result"]["isError"], false, "{}", responses[3]);
    assert!(stderr.is_empty(), "no diagnostics: {stderr}");
    eprintln!("mcp startup to initialize answer: {startup:?}");
    assert!(startup < Duration::from_secs(5));
    // The recipient reads it; an impostor with b's id and a's token is refused.
    let (inbox, _, _) = session(&home, &cwd, "i_b", &tb, &[call(1, "shinatama_inbox", serde_json::json!({}))]);
    let text = inbox[0]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("hello from a") && text.contains("untrusted=\\\"true\\\""), "{text}");
    let (refused, _, _) = session(&home, &cwd, "i_b", &ta, &[call(1, "shinatama_inbox", serde_json::json!({}))]);
    assert!(refused[0]["result"]["content"][0]["text"].as_str().unwrap().contains("not_authorized"));
    // Outside an instance: refused with exit 2 and nothing on stdout.
    let outside = Command::new(env!("CARGO_BIN_EXE_shinatama")).arg("mcp").env("SHINATAMA_HOME", &home).env_remove("SHINATAMA_INSTANCE_ID").current_dir(&cwd).output().unwrap();
    assert_eq!(outside.status.code(), Some(2));
    assert!(outside.stdout.is_empty());
    drop(store);
    let _ = std::fs::remove_dir_all(&base);
}
