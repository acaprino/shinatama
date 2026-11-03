//! Private bounded IPC between the terminal owner and its contained helper.
use shinatama_engine::resources::{job::OwnedJobKey, sampling::ProcessKey};
use std::{
    ffi::{OsStr, OsString},
    io::{self, Read, Write},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
};

pub const MAX_FRAME: usize = 128 * 1024;
pub const MAX_INPUT: usize = 65536;
pub const MAX_OUTPUT: usize = 8192;
pub const MAX_ERROR: usize = 1024;
/// Single argument-count bound shared by the owner-side configuration check,
/// the wire encoder/decoder and the contained command-line builder. Sealed
/// harness recipes (tracked Codex currently composes 21 arguments) must fit;
/// raising this bound is a deliberate, reviewed protocol change because both
/// endpoints enforce it independently.
pub const MAX_LAUNCH_ARGUMENTS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchConfig {
    pub rows: u16,
    pub cols: u16,
    pub cwd: PathBuf,
    pub executable: PathBuf,
    pub args: Vec<OsString>,
    pub owned: OwnedJobKey,
}
#[derive(Debug, PartialEq, Eq)]
pub enum HostCommand {
    Configure(LaunchConfig),
    AllowShell,
    ResumeShell,
    Input(Vec<u8>),
    Resize { rows: u16, cols: u16 },
    Close,
}
#[derive(Debug, PartialEq, Eq)]
pub enum HelperEvent {
    ConsoleReady,
    ShellReady(ProcessKey),
    Output(Vec<u8>),
    ShellClosing(u32),
    Exited(u32),
    Error(String),
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Invalid terminal helper frame")
}
fn size(rows: u16, cols: u16) -> io::Result<()> {
    if (2..=500).contains(&rows) && (2..=1000).contains(&cols) {
        Ok(())
    } else {
        Err(invalid())
    }
}
fn put_wide(out: &mut Vec<u8>, value: &OsStr, max: usize) -> io::Result<()> {
    let units: Vec<u16> = value.encode_wide().take(max + 1).collect();
    if units.len() > max || units.contains(&0) {
        return Err(invalid());
    }
    out.extend_from_slice(&(units.len() as u32).to_le_bytes());
    for unit in units {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    if out.len() > MAX_FRAME {
        return Err(invalid());
    }
    Ok(())
}
struct Decoder<'a>(&'a [u8]);
impl<'a> Decoder<'a> {
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        if n > self.0.len() {
            return Err(invalid());
        }
        let (value, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(value)
    }
    fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn wide(&mut self, max: usize) -> io::Result<OsString> {
        let count = self.u32()? as usize;
        if count > max {
            return Err(invalid());
        }
        let bytes = self.take(count.checked_mul(2).ok_or_else(invalid)?)?;
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|unit| u16::from_le_bytes([unit[0], unit[1]]))
            .collect();
        if units.contains(&0) {
            return Err(invalid());
        }
        Ok(OsString::from_wide(&units))
    }
    fn done(self) -> io::Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Trailing helper frame bytes={}", self.0.len()),
            ))
        }
    }
}
fn frame(out: &mut impl Write, kind: u8, data: &[u8]) -> io::Result<()> {
    if data.len() > MAX_FRAME {
        return Err(invalid());
    }
    out.write_all(&[kind])?;
    out.write_all(&(data.len() as u32).to_le_bytes())?;
    out.write_all(data)?;
    out.flush()
}
fn read_frame(input: &mut impl Read, host: bool) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut header = [0_u8; 5];
    loop {
        match input.read(&mut header[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    input.read_exact(&mut header[1..])?;
    let length = u32::from_le_bytes(header[1..].try_into().unwrap()) as usize;
    let limit = match (host, header[0]) {
        (true, 0) => MAX_FRAME,
        (true, 1 | 2 | 5) | (false, 64) => 0,
        (true, 3) => MAX_INPUT,
        (true, 4) | (false, 67 | 69) => 4,
        (false, 65) => 12,
        (false, 66) => MAX_OUTPUT,
        (false, 68) => MAX_ERROR,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unknown helper frame kind={} length={length}", header[0]),
            ));
        }
    };
    if length > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Oversized helper frame kind={} length={length} limit={limit}",
                header[0]
            ),
        ));
    }
    let mut data = vec![0; length];
    input.read_exact(&mut data)?;
    Ok(Some((header[0], data)))
}
fn config_payload(config: &LaunchConfig) -> io::Result<Vec<u8>> {
    size(config.rows, config.cols)?;
    if !config.cwd.is_absolute()
        || !config.executable.is_absolute()
        || config.args.len() > MAX_LAUNCH_ARGUMENTS
        || config.owned.job_key == [0; 16]
        || config.owned.owner.pid == 0
        || config.owned.owner.created_at_100ns == 0
    {
        return Err(invalid());
    }
    let mut data = Vec::new();
    data.extend_from_slice(&config.rows.to_le_bytes());
    data.extend_from_slice(&config.cols.to_le_bytes());
    data.extend_from_slice(&config.owned.job_key);
    data.extend_from_slice(&config.owned.owner.pid.to_le_bytes());
    data.extend_from_slice(&config.owned.owner.created_at_100ns.to_le_bytes());
    data.extend_from_slice(&config.owned.owner_windows_session_id.to_le_bytes());
    put_wide(&mut data, config.cwd.as_os_str(), 32767)?;
    put_wide(&mut data, config.executable.as_os_str(), 32767)?;
    data.extend_from_slice(&(config.args.len() as u16).to_le_bytes());
    for argument in &config.args {
        put_wide(&mut data, argument, 8192)?;
    }
    Ok(data)
}
pub fn write_command(out: &mut impl Write, command: &HostCommand) -> io::Result<()> {
    match command {
        HostCommand::Configure(config) => frame(out, 0, &config_payload(config)?),
        HostCommand::AllowShell => frame(out, 1, &[]),
        HostCommand::ResumeShell => frame(out, 2, &[]),
        HostCommand::Close => frame(out, 5, &[]),
        HostCommand::Input(data) if data.len() <= MAX_INPUT => frame(out, 3, data),
        HostCommand::Resize { rows, cols } => {
            size(*rows, *cols)?;
            frame(
                out,
                4,
                &[*rows, *cols]
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
                    .collect::<Vec<_>>(),
            )
        }
        _ => Err(invalid()),
    }
}
pub fn read_command(input: &mut impl Read) -> io::Result<Option<HostCommand>> {
    let Some((kind, data)) = read_frame(input, true)? else {
        return Ok(None);
    };
    let mut read = Decoder(&data);
    let command = match kind {
        0 => {
            let rows = read.u16()?;
            let cols = read.u16()?;
            size(rows, cols)?;
            let job_key = read.take(16)?.try_into().unwrap();
            let pid = read.u32()?;
            let created_at_100ns = read.u64()?;
            let owner_windows_session_id = read.u32()?;
            let cwd = PathBuf::from(read.wide(32767)?);
            let executable = PathBuf::from(read.wide(32767)?);
            let count = read.u16()?;
            if count as usize > MAX_LAUNCH_ARGUMENTS {
                return Err(invalid());
            }
            let mut args = Vec::new();
            for _ in 0..count {
                args.push(read.wide(8192)?);
            }
            let config = LaunchConfig {
                rows,
                cols,
                cwd,
                executable,
                args,
                owned: OwnedJobKey {
                    job_key,
                    owner: ProcessKey {
                        pid,
                        created_at_100ns,
                    },
                    owner_windows_session_id,
                },
            };
            config_payload(&config)?;
            HostCommand::Configure(config)
        }
        1 => HostCommand::AllowShell,
        2 => HostCommand::ResumeShell,
        3 => {
            let data = read.take(data.len())?.to_vec();
            HostCommand::Input(data)
        }
        4 => {
            let rows = read.u16()?;
            let cols = read.u16()?;
            size(rows, cols)?;
            HostCommand::Resize { rows, cols }
        }
        5 => HostCommand::Close,
        _ => return Err(invalid()),
    };
    read.done()?;
    Ok(Some(command))
}
pub fn write_event(out: &mut impl Write, event: &HelperEvent) -> io::Result<()> {
    match event {
        HelperEvent::ConsoleReady => frame(out, 64, &[]),
        HelperEvent::ShellReady(key) => {
            let mut data = key.pid.to_le_bytes().to_vec();
            data.extend_from_slice(&key.created_at_100ns.to_le_bytes());
            frame(out, 65, &data)
        }
        HelperEvent::Output(data) if data.len() <= MAX_OUTPUT => frame(out, 66, data),
        HelperEvent::Exited(code) => frame(out, 67, &code.to_le_bytes()),
        HelperEvent::ShellClosing(code) => frame(out, 69, &code.to_le_bytes()),
        HelperEvent::Error(message) if message.len() <= MAX_ERROR => {
            frame(out, 68, message.as_bytes())
        }
        _ => Err(invalid()),
    }
}
pub fn read_event(input: &mut impl Read) -> io::Result<Option<HelperEvent>> {
    let Some((kind, data)) = read_frame(input, false)? else {
        return Ok(None);
    };
    let mut read = Decoder(&data);
    let event = match kind {
        64 => HelperEvent::ConsoleReady,
        65 => {
            let key = ProcessKey {
                pid: read.u32()?,
                created_at_100ns: read.u64()?,
            };
            if key.pid == 0 || key.created_at_100ns == 0 {
                return Err(invalid());
            }
            HelperEvent::ShellReady(key)
        }
        66 => HelperEvent::Output(read.take(data.len())?.to_vec()),
        67 => HelperEvent::Exited(read.u32()?),
        69 => HelperEvent::ShellClosing(read.u32()?),
        68 => HelperEvent::Error(
            String::from_utf8(read.take(data.len())?.to_vec()).map_err(|_| invalid())?,
        ),
        _ => return Err(invalid()),
    };
    read.done()?;
    Ok(Some(event))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_config_and_control_frames_roundtrip_without_shell_interpolation() {
        let config = LaunchConfig {
            rows: 24,
            cols: 80,
            cwd: PathBuf::from("C:\\プロジェクト\\a & b"),
            executable: PathBuf::from("C:\\Windows\\shell.exe"),
            args: vec![OsString::from("literal $() & quoted \"value\"")],
            owned: OwnedJobKey {
                job_key: [1; 16],
                owner: ProcessKey {
                    pid: 1,
                    created_at_100ns: 2,
                },
                owner_windows_session_id: 0,
            },
        };
        let command = HostCommand::Configure(config);
        let mut bytes = Vec::new();
        write_command(&mut bytes, &command).unwrap();
        assert_eq!(read_command(&mut bytes.as_slice()).unwrap(), Some(command));
        let mut bytes = Vec::new();
        write_event(&mut bytes, &HelperEvent::Output(vec![0, 27, 255])).unwrap();
        assert_eq!(
            read_event(&mut bytes.as_slice()).unwrap(),
            Some(HelperEvent::Output(vec![0, 27, 255]))
        );
    }
    #[test]
    fn oversized_unknown_and_truncated_frames_fail_closed() {
        assert!(read_event(&mut [66, 1, 32, 0, 0].as_slice()).is_err());
        assert!(read_command(&mut [255, 0, 0, 0, 0].as_slice()).is_err());
        assert!(read_command(&mut [4, 4, 0, 0, 0, 24, 0].as_slice()).is_err());
        assert!(
            write_command(&mut Vec::new(), &HostCommand::Input(vec![0; MAX_INPUT + 1])).is_err()
        );
    }
    #[test]
    fn sealed_tracked_recipes_roundtrip_and_both_ends_share_one_argument_bound() {
        let base = LaunchConfig {
            rows: 24,
            cols: 80,
            cwd: PathBuf::from("C:\\synthetic\\project"),
            executable: PathBuf::from("C:\\synthetic\\codex.exe"),
            args: Vec::new(),
            owned: OwnedJobKey {
                job_key: [2; 16],
                owner: ProcessKey {
                    pid: 3,
                    created_at_100ns: 4,
                },
                owner_windows_session_id: 0,
            },
        };
        // The tracked Codex recipe: 15 fixed arguments plus 6 hook arguments
        // whose values embed quotes, regex metacharacters, TOML/JSON syntax
        // and a long base64 payload. Every layer must carry it unchanged.
        let mut tracked = base.clone();
        tracked.args = (0..15)
            .map(|index| OsString::from(format!("--fixed{index}=\"unelevated\"")))
            .chain([
                OsString::from("-c"),
                OsString::from("features.hooks=true"),
                OsString::from("-c"),
                OsString::from(
                    "hooks.SessionStart=[{matcher=\"^(startup|resume|clear|compact|fork)$\",hooks=[{type=\"command\",command=\"C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe -NoProfile -NonInteractive -EncodedCommand JABFAHIAcgBvAHIAQQBjAHQAaQBvAG4A\",timeout=5,async=false}]}]",
                ),
                OsString::from("-c"),
                OsString::from(
                    "hooks.state.\"C:\\\\<session-flags>\\\\config.toml:session_start:0:0\"={enabled=true,trusted_hash=\"sha256:0000000000000000000000000000000000000000000000000000000000000000\"}",
                ),
            ])
            .collect();
        assert_eq!(tracked.args.len(), 21);
        let command = HostCommand::Configure(tracked.clone());
        let mut bytes = Vec::new();
        write_command(&mut bytes, &command).unwrap();
        assert_eq!(read_command(&mut bytes.as_slice()).unwrap(), Some(command));
        // One bound, both endpoints: the shared limit roundtrips, one more
        // argument fails closed on encode and on decode.
        let mut at_bound = base.clone();
        at_bound.args = (0..MAX_LAUNCH_ARGUMENTS)
            .map(|index| OsString::from(format!("-c{index}")))
            .collect();
        let command = HostCommand::Configure(at_bound);
        let mut bytes = Vec::new();
        write_command(&mut bytes, &command).unwrap();
        assert_eq!(read_command(&mut bytes.as_slice()).unwrap(), Some(command));
        let mut over_bound = base;
        over_bound.args = (0..MAX_LAUNCH_ARGUMENTS + 1)
            .map(|index| OsString::from(format!("-c{index}")))
            .collect();
        assert!(write_command(&mut Vec::new(), &HostCommand::Configure(over_bound)).is_err());
    }
}
