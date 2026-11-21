//! Reads the shell's working directory from its output.
//!
//! Shells report it with OSC 7 (`ESC ] 7 ; file://host/path BEL`, bash, zsh,
//! fish, oh-my-posh) or Windows Terminal's OSC 9;9. On Windows ConPTY drops
//! both and only forwards the window title, so castorterm's PowerShell
//! integration (`shell::powershell_integration`) sets the title to
//! [`TITLE_PREFIX`] + path after each prompt. `alacritty_terminal` ignores
//! all of this, so [`TapPty`] wraps the PTY and scans the bytes on their way
//! to the terminal, without changing them.

use std::io::{self, Read};
use std::sync::Arc;

use alacritty_terminal::event::{OnResize, WindowSize};
use alacritty_terminal::tty::{self, ChildEvent, EventedPty, EventedReadWrite};
use polling::{Event, PollMode, Poller};

/// Title prefix the PowerShell integration uses to report the location.
pub const TITLE_PREFIX: &str = "castorterm:cwd:";

/// Longest OSC payload kept; longer ones are dropped.
const MAX_OSC: usize = 4096;

#[derive(Debug, Default)]
enum State {
    #[default]
    Ground,
    Escape,
    Osc,
    /// ESC seen inside an OSC: `\` ends it (ST).
    OscEscape,
}

/// Incremental scanner for OSC 7 and OSC 9;9 across reads.
#[derive(Debug, Default)]
pub struct OscScanner {
    state: State,
    payload: Vec<u8>,
    overflow: bool,
}

impl OscScanner {
    /// Feeds bytes; returns the working directories reported in them.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut found = Vec::new();
        for &b in bytes {
            self.state = match (&self.state, b) {
                (State::Ground, 0x1b) => State::Escape,
                (State::Ground, _) => State::Ground,
                (State::Escape, b']') => {
                    self.payload.clear();
                    self.overflow = false;
                    State::Osc
                }
                (State::Escape, 0x1b) => State::Escape,
                (State::Escape, _) => State::Ground,
                (State::Osc, 0x07) => {
                    found.extend(self.finish());
                    State::Ground
                }
                (State::Osc, 0x1b) => State::OscEscape,
                (State::Osc, _) => {
                    if self.payload.len() < MAX_OSC {
                        self.payload.push(b);
                    } else {
                        self.overflow = true;
                    }
                    State::Osc
                }
                (State::OscEscape, b'\\') => {
                    found.extend(self.finish());
                    State::Ground
                }
                // Not ST: the OSC was cut short; a new sequence may start.
                (State::OscEscape, b']') => {
                    self.payload.clear();
                    self.overflow = false;
                    State::Osc
                }
                (State::OscEscape, _) => State::Ground,
            };
        }
        found
    }

    fn finish(&mut self) -> Option<String> {
        if self.overflow {
            return None;
        }
        let text = std::str::from_utf8(&self.payload).ok()?;
        cwd_from_osc(text)
    }
}

/// Working directory in an OSC payload (without `ESC ]` and terminator).
pub fn cwd_from_osc(payload: &str) -> Option<String> {
    let path = if let Some(url) = payload.strip_prefix("7;") {
        // file://host/path, percent-encoded; the host is ignored.
        let rest = url.strip_prefix("file://")?;
        let path = &rest[rest.find('/')?..];
        let decoded = percent_decode(path)?;
        // file:///C:/x on Windows.
        match decoded.as_bytes() {
            [b'/', drive, b':', ..] if drive.is_ascii_alphabetic() => decoded[1..].to_string(),
            _ => decoded,
        }
    } else if let Some(path) = payload.strip_prefix("9;9;") {
        path.trim_matches('"').to_string()
    } else {
        let title = payload
            .strip_prefix("0;")
            .or_else(|| payload.strip_prefix("2;"))?;
        title.strip_prefix(TITLE_PREFIX)?.to_string()
    };
    let clean = !path.is_empty() && !path.chars().any(char::is_control);
    clean.then_some(path)
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

type Reader = <tty::Pty as EventedReadWrite>::Reader;

/// Reader that passes bytes through unchanged and scans them.
pub struct TapReader {
    /// The wrapped PTY's reader. Points into `TapPty::inner`, a `Box` that
    /// is never moved or dropped while this reader exists (both live in the
    /// same `TapPty`, and `inner` is declared after `reader` so it drops
    /// last).
    inner: *mut Reader,
    scanner: OscScanner,
    on_cwd: Box<dyn FnMut(String) + Send>,
}

// SAFETY: the pointer is only used from the thread that owns the `TapPty`
// (the PTY event loop), exactly like the reader it points to.
unsafe impl Send for TapReader {}

impl Read for TapReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // SAFETY: see `inner`.
        let n = unsafe { (*self.inner).read(buf)? };
        for cwd in self.scanner.feed(&buf[..n]) {
            (self.on_cwd)(cwd);
        }
        Ok(n)
    }
}

/// The PTY with its output scanned for working directory reports.
pub struct TapPty {
    reader: TapReader,
    inner: Box<tty::Pty>,
}

impl TapPty {
    pub fn new(pty: tty::Pty, on_cwd: impl FnMut(String) + Send + 'static) -> Self {
        let mut inner = Box::new(pty);
        let ptr: *mut Reader = inner.reader();
        Self {
            reader: TapReader {
                inner: ptr,
                scanner: OscScanner::default(),
                on_cwd: Box::new(on_cwd),
            },
            inner,
        }
    }
}

impl EventedReadWrite for TapPty {
    type Reader = TapReader;
    type Writer = <tty::Pty as EventedReadWrite>::Writer;

    unsafe fn register(
        &mut self,
        poll: &Arc<Poller>,
        event: Event,
        mode: PollMode,
    ) -> io::Result<()> {
        // SAFETY: forwarded; the caller upholds the same contract.
        unsafe { self.inner.register(poll, event, mode) }
    }

    fn reregister(&mut self, poll: &Arc<Poller>, event: Event, mode: PollMode) -> io::Result<()> {
        self.inner.reregister(poll, event, mode)
    }

    fn deregister(&mut self, poll: &Arc<Poller>) -> io::Result<()> {
        self.inner.deregister(poll)
    }

    fn reader(&mut self) -> &mut TapReader {
        &mut self.reader
    }

    fn writer(&mut self) -> &mut Self::Writer {
        self.inner.writer()
    }
}

impl EventedPty for TapPty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.inner.next_child_event()
    }
}

impl OnResize for TapPty {
    fn on_resize(&mut self, window_size: WindowSize) {
        self.inner.on_resize(window_size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc7_and_osc99() {
        assert_eq!(
            cwd_from_osc("7;file://host/home/me/src").as_deref(),
            Some("/home/me/src")
        );
        assert_eq!(
            cwd_from_osc("7;file:///C:/Users/me%20x").as_deref(),
            Some("C:/Users/me x")
        );
        assert_eq!(
            cwd_from_osc("9;9;D:\\proyectos").as_deref(),
            Some("D:\\proyectos")
        );
        assert_eq!(cwd_from_osc("9;9;\"C:\\a b\"").as_deref(), Some("C:\\a b"));
        assert_eq!(cwd_from_osc("0;title"), None);
        assert_eq!(
            cwd_from_osc("0;castorterm:cwd:D:\\src\\app").as_deref(),
            Some("D:\\src\\app")
        );
        assert_eq!(
            cwd_from_osc("2;castorterm:cwd:/tmp").as_deref(),
            Some("/tmp")
        );
        assert_eq!(cwd_from_osc("0;castorterm:cwd:"), None);
        assert_eq!(cwd_from_osc("7;http://x/y"), None);
        assert_eq!(cwd_from_osc("7;file://host/a%0Ab"), None);
        assert_eq!(cwd_from_osc("7;file://host/a%zz"), None);
    }

    #[test]
    fn scanner_across_reads_and_terminators() {
        let mut s = OscScanner::default();
        assert!(s.feed(b"hello \x1b]7;file://h/tmp/a").is_empty());
        assert_eq!(s.feed(b"bc\x07 world"), vec!["/tmp/abc".to_string()]);
        // ST terminator and OSC 9;9.
        assert_eq!(s.feed(b"\x1b]9;9;C:\\x\x1b\\"), vec!["C:\\x".to_string()]);
        // Other sequences are ignored.
        assert!(s.feed(b"\x1b[31mred\x1b]0;title\x07").is_empty());
        // Oversized payloads are dropped.
        let mut big = b"\x1b]7;file://h/".to_vec();
        big.extend(std::iter::repeat_n(b'a', MAX_OSC * 2));
        big.push(0x07);
        assert!(s.feed(&big).is_empty());
        assert_eq!(s.feed(b"\x1b]7;file://h/ok\x07"), vec!["/ok".to_string()]);
    }
}
