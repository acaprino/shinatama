//! Shinatama defines Codex's Mukade hooks itself (`shinatama_engine::mukade_chat::codex_hooks`) and
//! relies on the approval `mukade chat trust codex` records for Mukade's own definitions. Codex
//! trusts a hook by the hash of its definition, so the two must be identical: these are the lines
//! and timeouts Shinatama writes, checked against Mukade's own kit at the pinned revision.
use mukade::chat::adapter::{Kit, STOP_HOOK_TIMEOUT};

fn kit(exe: &str) -> Kit {
    Kit {
        exe: exe.into(),
        folder: Default::default(),
        server: mukade::chat::connect::Server {
            endpoint: String::new(),
            token: String::new(),
            service_id: String::new(),
        },
        stop_timeout: STOP_HOOK_TIMEOUT,
        args: Vec::new(),
    }
}

#[test]
fn mukade_s_codex_hook_lines_and_stop_timeout_are_the_ones_shinatama_writes() {
    // shinatama_engine::mukade_chat::CODEX_STOP_HOOK_TIMEOUT
    assert_eq!(STOP_HOOK_TIMEOUT, 3630);
    // The program path Shinatama hands Mukade, with backslashes; both write forward slashes.
    let kit = kit(r"C:\Shinatama\mukade\mukade.exe");
    assert_eq!(
        kit.powershell_hook_command("codex", "prompt"),
        "& 'C:/Shinatama/mukade/mukade.exe' chat hook codex prompt"
    );
    assert_eq!(
        kit.powershell_hook_command("codex", "stop"),
        "& 'C:/Shinatama/mukade/mukade.exe' chat hook codex stop"
    );
    let quoted = self::kit(r"C:\O'Brien\mukade\mukade.exe");
    assert_eq!(
        quoted.powershell_hook_command("codex", "stop"),
        "& 'C:/O''Brien/mukade/mukade.exe' chat hook codex stop"
    );
}
