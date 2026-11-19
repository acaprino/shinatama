//! A real PTY with synthetic output only: no agent and no model calls.
use anyhow::{Result, bail};
use shinatama_terminal::TerminalHost;
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn main() -> Result<()> {
    let host = TerminalHost::default();
    let session = host.start(Path::new("."), 24, 80)?;
    host.resize(&session.id, 32, 100)?;
    #[cfg(windows)]
    let input = "Write-Output ('SHINATAMA_' + 'PTY_OK'); exit\r";
    #[cfg(not(windows))]
    let input = "printf 'SHINATAMA_%s\\n' PTY_OK; exit\n";
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut output = Vec::new();
    let mut exited = false;
    let mut sent = !cfg!(windows);
    if sent {
        host.write(&session.id, input.as_bytes().to_vec())?;
    }
    while Instant::now() < deadline {
        let batch = host.read(&session.id)?;
        for chunk in batch.chunks {
            output.extend(chunk);
            // ConPTY requests the emulator cursor before starting the Windows shell.
            // The desktop's xterm answers this; this headless probe must do so too.
            if output.ends_with(b"\x1b[6n") {
                host.write(&session.id, b"\x1b[1;1R".to_vec())?;
            }
        }
        if !sent
            && String::from_utf8_lossy(&output).contains(if cfg!(windows) { "PS " } else { "$" })
        {
            host.write(&session.id, input.as_bytes().to_vec())?;
            sent = true;
        }
        if let Some(error) = batch.error {
            bail!("PTY read failed: {error}");
        }
        if batch.eof && batch.exit_code.is_some() {
            exited = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    host.stop(&session.id)?;
    if !exited || !String::from_utf8_lossy(&output).contains("SHINATAMA_PTY_OK") {
        bail!(
            "PTY smoke failed: exited={exited}, sent={sent}, output={:?}",
            String::from_utf8_lossy(&output)
        );
    }
    assert!(host.list().is_empty());
    println!(
        "PASS: real shell, input, output, resize, exit, cleanup ({} bytes)",
        output.len()
    );
    Ok(())
}
