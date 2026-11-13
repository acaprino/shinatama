//! The framing of an export between the contained worker and its parent (ADR-0053 v2).
//!
//! An export of any size cannot be one bounded reply. The worker writes its rendered body to the
//! pipe in frames of at most `MAX_BODY_FRAME` bytes, then one final frame (JSON: the report, the
//! header that goes in front of the body, the attachments, or the refusal). There is no total
//! bound: the parent holds one frame at a time, hands each body frame to its sink and commits
//! only after the final frame. Every other worker reply stays one bounded JSON document.
//!
//! A third kind, a progress frame, tells the parent how far a long read has come: entries read,
//! body bytes written and hits so far. The worker sends at most one every `PROGRESS_INTERVAL`.
//! A search is framed the same way (progress frames, then one final frame holding its ordinary
//! reply), so the parent can show both.
//!
//! A frame is `tag (1 byte) | length (u32, little endian) | payload`.
use serde::Serialize;
use std::io::{self, Write};
use std::time::{Duration, Instant};

pub const TAG_BODY: u8 = 1;
pub const TAG_FINAL: u8 = 2;
pub const TAG_PROGRESS: u8 = 3;
/// A progress frame is three little-endian u64: entries, body bytes, hits.
pub const PROGRESS_FRAME: usize = 24;
/// The worker sends progress at most this often (the parent forwards each one to the UI).
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// How far a worker has come. It is context for a person, never a percentage: a session's entries
/// are not known before it is read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub entries: u64,
    pub bytes: u64,
    pub hits: u64,
}

/// What a parsed frame gives the parent.
pub enum Event<'a> {
    Body(&'a [u8]),
    Progress(Progress),
}
/// The most one body frame carries.
pub const MAX_BODY_FRAME: usize = 1024 * 1024;
/// The final frame carries the report, the header and the attachments (at most 12 MiB, base64:
/// 16 MiB, and bounded one by one, not by the session), never the session's text.
pub const MAX_FINAL_FRAME: usize = 24 * 1024 * 1024;
const HEADER: usize = 5;

#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    /// A tag this version does not know, or a frame longer than its tag allows.
    Malformed,
    /// Bytes after the final frame.
    Trailing,
}

/// The worker's end: bytes in, frames out.
pub struct FrameWriter<W: Write> {
    out: W,
    pending: Vec<u8>,
    body_bytes: u64,
    last_progress: Instant,
    interval: Duration,
}

impl<W: Write> FrameWriter<W> {
    pub fn new(out: W) -> Self {
        Self::with_interval(out, PROGRESS_INTERVAL)
    }

    pub fn with_interval(out: W, interval: Duration) -> Self {
        Self {
            out,
            pending: Vec::with_capacity(MAX_BODY_FRAME),
            body_bytes: 0,
            last_progress: Instant::now(),
            interval,
        }
    }

    /// Sends a progress frame if the interval has passed since the last one (or since the start:
    /// a read that ends sooner sends none). Cheap to call per entry.
    pub fn progress(&mut self, entries: u64, hits: u64) -> io::Result<()> {
        if self.last_progress.elapsed() < self.interval {
            return Ok(());
        }
        self.last_progress = Instant::now();
        let mut payload = [0u8; PROGRESS_FRAME];
        payload[..8].copy_from_slice(&entries.to_le_bytes());
        payload[8..16].copy_from_slice(&self.body_bytes.to_le_bytes());
        payload[16..].copy_from_slice(&hits.to_le_bytes());
        self.frame(TAG_PROGRESS, false, &payload)
    }

    fn frame(&mut self, tag: u8, payload_from_pending: bool, payload: &[u8]) -> io::Result<()> {
        let length = if payload_from_pending { self.pending.len() } else { payload.len() };
        let mut header = [tag, 0, 0, 0, 0];
        header[1..].copy_from_slice(&(length as u32).to_le_bytes());
        self.out.write_all(&header)?;
        if payload_from_pending {
            self.out.write_all(&self.pending)?;
            self.pending.clear();
        } else {
            self.out.write_all(payload)?;
        }
        Ok(())
    }

    /// Appends body bytes; a full frame is written as soon as there is one.
    pub fn write_body(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        self.body_bytes += bytes.len() as u64;
        while !bytes.is_empty() {
            let room = MAX_BODY_FRAME - self.pending.len();
            let (now, rest) = bytes.split_at(room.min(bytes.len()));
            self.pending.extend_from_slice(now);
            bytes = rest;
            if self.pending.len() == MAX_BODY_FRAME {
                self.frame(TAG_BODY, true, &[])?;
            }
        }
        Ok(())
    }

    /// The body bytes written so far.
    pub fn body_bytes(&self) -> u64 {
        self.body_bytes
    }

    /// Flushes the last body frame and writes the final frame; nothing follows it.
    pub fn finish(&mut self, final_frame: &[u8]) -> io::Result<()> {
        if final_frame.len() > MAX_FINAL_FRAME {
            return Err(io::Error::other("final frame over its bound"));
        }
        if !self.pending.is_empty() {
            self.frame(TAG_BODY, true, &[])?;
        }
        self.frame(TAG_FINAL, false, final_frame)?;
        self.out.flush()
    }
}

/// The parent's end: bytes in, whole frames out. It holds at most one frame.
#[derive(Default)]
pub struct FrameParser {
    final_frame: Option<Vec<u8>>,
    body_bytes: u64,
}

impl FrameParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Consumes every whole frame at the front of `buffer`, giving each body or progress frame to
    /// `on_event` (whose error stops the parse and is returned as `Err(Err(e))`). What remains in
    /// `buffer` is the start of the next frame, at most one frame plus a header.
    pub fn take<E>(
        &mut self,
        buffer: &mut Vec<u8>,
        on_event: &mut dyn FnMut(Event<'_>) -> Result<(), E>,
    ) -> Result<(), Result<FrameError, E>> {
        let mut at = 0usize;
        let outcome = loop {
            let rest = &buffer[at..];
            if self.final_frame.is_some() {
                if rest.is_empty() {
                    break Ok(());
                }
                break Err(Ok(FrameError::Trailing));
            }
            if rest.len() < HEADER {
                break Ok(());
            }
            let tag = rest[0];
            let length = u32::from_le_bytes([rest[1], rest[2], rest[3], rest[4]]) as usize;
            let bound = match tag {
                TAG_BODY => MAX_BODY_FRAME,
                TAG_FINAL => MAX_FINAL_FRAME,
                TAG_PROGRESS => PROGRESS_FRAME,
                _ => break Err(Ok(FrameError::Malformed)),
            };
            if length > bound
                || (tag == TAG_BODY && length == 0)
                || (tag == TAG_PROGRESS && length != PROGRESS_FRAME)
            {
                break Err(Ok(FrameError::Malformed));
            }
            if rest.len() < HEADER + length {
                break Ok(());
            }
            let payload = &rest[HEADER..HEADER + length];
            if tag == TAG_BODY {
                if let Err(error) = on_event(Event::Body(payload)) {
                    break Err(Err(error));
                }
                self.body_bytes += length as u64;
            } else if tag == TAG_PROGRESS {
                let field = |at: usize| u64::from_le_bytes(payload[at..at + 8].try_into().expect("8 bytes"));
                let progress = Progress { entries: field(0), bytes: field(8), hits: field(16) };
                if let Err(error) = on_event(Event::Progress(progress)) {
                    break Err(Err(error));
                }
            } else {
                self.final_frame = Some(payload.to_vec());
            }
            at += HEADER + length;
        };
        buffer.drain(..at);
        outcome
    }

    /// The final frame's payload, once it has arrived.
    pub fn final_frame(&self) -> Option<&[u8]> {
        self.final_frame.as_deref()
    }

    pub fn body_bytes(&self) -> u64 {
        self.body_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(body: &[u8], final_frame: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut writer = FrameWriter::new(&mut out);
        writer.write_body(body).unwrap();
        writer.finish(final_frame).unwrap();
        out
    }

    #[test]
    fn frames_are_bounded_round_trip_and_arrive_in_any_split() {
        let body: Vec<u8> = (0..(2 * MAX_BODY_FRAME + 1234)).map(|i| (i % 251) as u8).collect();
        let bytes = wire(&body, b"{\"done\":true}");
        // Three body frames (1 MiB, 1 MiB, 1234) then the final one.
        assert_eq!(bytes.len(), body.len() + 3 * HEADER + HEADER + 13);
        for chunk in [7usize, 16 * 1024, bytes.len()] {
            let mut parser = FrameParser::new();
            let mut buffer = Vec::new();
            let mut got = Vec::new();
            for piece in bytes.chunks(chunk) {
                buffer.extend_from_slice(piece);
                parser
                    .take::<()>(&mut buffer, &mut |event| {
                        if let Event::Body(frame) = event {
                            assert!(frame.len() <= MAX_BODY_FRAME);
                            got.extend_from_slice(frame);
                        }
                        Ok(())
                    })
                    .unwrap();
                assert!(buffer.len() <= MAX_BODY_FRAME + HEADER, "at most one frame is held");
            }
            assert_eq!(got, body);
            assert_eq!(parser.final_frame(), Some(&b"{\"done\":true}"[..]));
            assert_eq!(parser.body_bytes(), body.len() as u64);
            assert!(buffer.is_empty());
        }
    }

    #[test]
    fn unknown_tags_oversized_frames_and_trailing_bytes_are_refused() {
        let mut parser = FrameParser::new();
        let mut buffer = vec![9, 1, 0, 0, 0, 0];
        assert!(matches!(parser.take::<()>(&mut buffer, &mut |_| Ok(())), Err(Ok(FrameError::Malformed))));
        let mut parser = FrameParser::new();
        let mut buffer = vec![TAG_BODY];
        buffer.extend_from_slice(&((MAX_BODY_FRAME as u32) + 1).to_le_bytes());
        assert!(matches!(parser.take::<()>(&mut buffer, &mut |_| Ok(())), Err(Ok(FrameError::Malformed))));
        let mut parser = FrameParser::new();
        let mut buffer = wire(b"abc", b"{}");
        buffer.push(0);
        assert!(matches!(parser.take::<()>(&mut buffer, &mut |_| Ok(())), Err(Ok(FrameError::Trailing))));
    }

    #[test]
    fn progress_frames_are_bounded_in_rate_and_carry_the_counts() {
        let mut out = Vec::new();
        {
            let mut writer = FrameWriter::new(&mut out);
            writer.write_body(&[7u8; 5000]).unwrap();
            // Called once per entry, as fast as it likes, for 1.1 s: at most one per 250 ms.
            let started = Instant::now();
            let mut entries = 0u64;
            while started.elapsed() < Duration::from_millis(1100) {
                entries += 1;
                writer.progress(entries, entries / 10).unwrap();
                std::thread::sleep(Duration::from_millis(1));
            }
            writer.finish(b"{}").unwrap();
        }
        let mut parser = FrameParser::new();
        let mut buffer = out;
        let mut seen = Vec::new();
        parser
            .take::<()>(&mut buffer, &mut |event| {
                if let Event::Progress(progress) = event {
                    seen.push(progress);
                }
                Ok(())
            })
            .unwrap();
        assert!((3..=4).contains(&seen.len()), "{} frames in 1.1 s", seen.len());
        assert!(seen.windows(2).all(|pair| pair[0].entries < pair[1].entries));
        assert!(seen.iter().all(|p| p.bytes == 5000 && p.hits == p.entries / 10));
        assert!(parser.final_frame().is_some());
        // A progress frame of any other length is refused.
        let mut parser = FrameParser::new();
        let mut buffer = vec![TAG_PROGRESS, 8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
        assert!(matches!(parser.take::<()>(&mut buffer, &mut |_| Ok(())), Err(Ok(FrameError::Malformed))));
    }

    #[test]
    fn a_failing_sink_stops_the_parse_with_its_own_error() {
        let mut parser = FrameParser::new();
        let mut buffer = wire(b"abc", b"{}");
        let outcome = parser.take(&mut buffer, &mut |_| Err("disk full"));
        assert!(matches!(outcome, Err(Err("disk full"))));
    }
}
