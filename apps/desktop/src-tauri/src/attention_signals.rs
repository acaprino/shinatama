//! Observes terminal notification controls without retaining their payloads.
//!
//! BEL and OSC 9/777 say only that a terminal requested attention. They do not
//! prove an approval, question, or completed turn. Callers must use generic copy
//! and must never authorize an action from these signals. This observer does not
//! consume or change the bytes sent to the terminal emulator.
//!
//! Protocol references:
//! https://iterm2.com/documentation-escape-codes.html (OSC 9 and 9;4 progress)
//! https://wezterm.org/escape-sequences.html (OSC 777;notify;title;body)

const MAX_OSC_BYTES: usize = 4096;
const OSC9: &[u8] = b"9;";
const OSC777: &[u8] = b"777;notify;";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Bell,
    Notification,
}

#[derive(Clone, Copy, Default)]
enum State {
    #[default]
    Ground,
    Escape,
    Csi,
    Osc,
    OscEscape,
    IgnoredString,
    IgnoredStringEscape,
}

#[derive(Default)]
struct Osc {
    bytes: usize,
    header_index: usize,
    candidates: u8,
    kind: u8,
    payload_bytes: usize,
    first_is_four: bool,
    progress: bool,
    separator: bool,
    title: bool,
    body: bool,
    invalid: bool,
}

impl Osc {
    fn new() -> Self {
        Self {
            candidates: 3,
            ..Self::default()
        }
    }

    fn push(&mut self, byte: u8) {
        self.bytes = self.bytes.saturating_add(1).min(MAX_OSC_BYTES + 1);
        if self.bytes > MAX_OSC_BYTES || byte < 0x20 || byte == 0x7f {
            self.invalid = true;
        }
        if self.invalid {
            return;
        }
        match self.kind {
            0 => {
                if OSC9.get(self.header_index) != Some(&byte) {
                    self.candidates &= !1;
                }
                if OSC777.get(self.header_index) != Some(&byte) {
                    self.candidates &= !2;
                }
                self.header_index += 1;
                if self.candidates == 0 {
                    self.invalid = true;
                } else if self.candidates & 1 != 0 && self.header_index == OSC9.len() {
                    self.kind = 9;
                } else if self.candidates & 2 != 0 && self.header_index == OSC777.len() {
                    self.kind = 7;
                }
            }
            9 => {
                if self.payload_bytes == 0 {
                    self.first_is_four = byte == b'4';
                } else if self.payload_bytes == 1 && self.first_is_four && byte == b';' {
                    self.progress = true;
                }
                self.payload_bytes += 1;
            }
            7 => {
                if !self.separator && byte == b';' {
                    self.separator = true;
                } else if self.separator {
                    self.body = true;
                } else {
                    self.title = true;
                }
            }
            _ => unreachable!(),
        }
    }

    fn is_notification(&self) -> bool {
        if self.invalid {
            return false;
        }
        match self.kind {
            9 => {
                self.payload_bytes > 0
                    && !self.progress
                    && !(self.first_is_four && self.payload_bytes == 1)
            }
            7 => self.separator && (self.title || self.body),
            _ => false,
        }
    }
}

/// Keep one observer per PTY. Memory is constant, including malformed or
/// unterminated strings; only counters and protocol flags survive a feed call.
/// Raw C1 bytes are deliberately not interpreted in UTF-8 terminal output.
#[derive(Default)]
pub struct AttentionSignals {
    state: State,
    osc: Osc,
}

impl AttentionSignals {
    /// Coalesces repeated controls to at most one signal of each kind per call.
    /// Callers additionally rate-limit across reads/terminals before alerting.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Signal> {
        let mut signals = Vec::new();
        for &byte in bytes {
            // CAN/SUB cancel a control string; they never complete a notice.
            if matches!(byte, 0x18 | 0x1a) {
                self.state = State::Ground;
                self.osc = Osc::default();
                continue;
            }
            match self.state {
                State::Ground => match byte {
                    0x1b => self.state = State::Escape,
                    0x07 => emit(&mut signals, Signal::Bell),
                    _ => {}
                },
                State::Escape => {
                    self.state = match byte {
                        b']' => {
                            self.osc = Osc::new();
                            State::Osc
                        }
                        b'P' | b'X' | b'^' | b'_' => State::IgnoredString,
                        b'[' => State::Csi,
                        0x1b => State::Escape,
                        0x07 => {
                            emit(&mut signals, Signal::Bell);
                            State::Ground
                        }
                        _ => State::Ground,
                    };
                }
                State::Csi => match byte {
                    0x1b => self.state = State::Escape,
                    0x07 => emit(&mut signals, Signal::Bell),
                    0x40..=0x7e => self.state = State::Ground,
                    _ => {}
                },
                State::Osc => match byte {
                    0x07 => self.finish_osc(&mut signals),
                    0x1b => self.state = State::OscEscape,
                    _ => self.osc.push(byte),
                },
                State::OscEscape => {
                    if byte == b'\\' {
                        self.finish_osc(&mut signals);
                    } else {
                        // Malformed embedded escapes cannot become a second
                        // notification or turn an OSC terminator into a bell.
                        self.osc.invalid = true;
                        if byte == 0x07 {
                            self.finish_osc(&mut signals);
                        } else if byte != 0x1b {
                            self.state = State::Osc;
                        }
                    }
                }
                State::IgnoredString => {
                    if byte == 0x1b {
                        self.state = State::IgnoredStringEscape;
                    }
                }
                State::IgnoredStringEscape => {
                    self.state = match byte {
                        b'\\' => State::Ground,
                        0x1b => State::IgnoredStringEscape,
                        _ => State::IgnoredString,
                    };
                }
            }
        }
        signals
    }

    fn finish_osc(&mut self, signals: &mut Vec<Signal>) {
        if self.osc.is_notification() {
            emit(signals, Signal::Notification);
        }
        self.osc = Osc::default();
        self.state = State::Ground;
    }
}

fn emit(signals: &mut Vec<Signal>, signal: Signal) {
    if !signals.contains(&signal) {
        signals.push(signal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_notifications_at_every_chunk_boundary() {
        for sequence in [
            &b"\x1b]9;private message\x07"[..],
            &b"\x1b]9;private message\x1b\\"[..],
            &b"\x1b]777;notify;private title;private body\x07"[..],
            &b"\x1b]777;notify;private title;private body\x1b\\"[..],
        ] {
            for split in 0..=sequence.len() {
                let mut observer = AttentionSignals::default();
                let mut signals = observer.feed(&sequence[..split]);
                signals.extend(observer.feed(&sequence[split..]));
                assert_eq!(signals, vec![Signal::Notification], "split {split}");
            }
        }
    }

    #[test]
    fn never_infers_requests_or_completion_from_text() {
        let mut observer = AttentionSignals::default();
        assert!(
            observer
                .feed(b"Approval requested: Allow? Finished. Waiting for input.\r\n")
                .is_empty()
        );
        assert_eq!(observer.feed(b"\x07"), vec![Signal::Bell]);
    }

    #[test]
    fn ignores_other_osc_commands_and_progress_terminators() {
        let mut observer = AttentionSignals::default();
        for sequence in [
            &b"\x1b]0;private window title\x07"[..],
            &b"\x1b]52;c;c2VjcmV0\x07"[..],
            &b"\x1b]9;4;1;50\x07"[..],
            &b"\x1b]9;4\x1b\\"[..],
            &b"\x1b]777;not-notify;title;body\x07"[..],
            &b"\x1b]777;notify;missing-body-separator\x07"[..],
            &b"\x1b]9;\x07"[..],
        ] {
            assert!(observer.feed(sequence).is_empty());
        }
        assert_eq!(
            observer.feed(b"\x1b]9;4 results\x07"),
            vec![Signal::Notification]
        );
    }

    #[test]
    fn discards_control_strings_including_tmux_passthrough() {
        for start in *b"PX^_" {
            let mut observer = AttentionSignals::default();
            observer.feed(&[0x1b, start]);
            assert!(observer.feed(b"\x07\x1b]9;embedded\x07\x1b\\").is_empty());
            assert_eq!(observer.feed(b"\x07"), vec![Signal::Bell]);
        }
    }

    #[test]
    fn overlong_and_malformed_osc_never_emit_bells_on_termination() {
        let mut observer = AttentionSignals::default();
        observer.feed(b"\x1b]9;");
        for _ in 0..10 {
            assert!(observer.feed(&[b'x'; 1024]).is_empty());
        }
        assert!(observer.feed(b"\x07").is_empty());
        assert!(observer.feed(b"\x1b]9;bad\x1b]9;injected\x07").is_empty());
        assert_eq!(
            observer.feed(b"\x1b]9;valid\x07"),
            vec![Signal::Notification]
        );
    }

    #[test]
    fn cancellation_does_not_deliver_partial_notification() {
        let mut observer = AttentionSignals::default();
        assert!(observer.feed(b"\x1b]9;partial\x18").is_empty());
        assert_eq!(observer.feed(b"\x07"), vec![Signal::Bell]);
    }

    #[test]
    fn output_is_bounded_even_for_notification_floods() {
        let mut observer = AttentionSignals::default();
        let bytes = b"\x07\x1b]9;notice\x07".repeat(10_000);
        assert_eq!(
            observer.feed(&bytes),
            vec![Signal::Bell, Signal::Notification]
        );
        assert!(std::mem::size_of::<AttentionSignals>() < 128);
    }
}
