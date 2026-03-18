//! Channel-based line reader with per-line idle timeout.
//!
//! Wraps a `BufRead` in a dedicated thread that sends lines through a channel.
//! The consumer uses `recv_timeout` to detect when the source has gone idle
//! (no output for longer than the idle timeout).

use std::io::BufRead;
use std::sync::mpsc;
use std::time::Duration;

/// A line reader that yields lines with an idle timeout between them.
///
/// If no line arrives within `idle_timeout`, `next_line()` returns
/// `Err(IdleTimeout)`. The background reader thread runs until EOF or
/// read error; it is not explicitly joined (it will exit when the source
/// closes).
pub struct TimedLineReader {
    rx: mpsc::Receiver<std::io::Result<String>>,
    idle_timeout: Duration,
}

/// Error from `next_line()`.
#[derive(Debug)]
#[allow(dead_code)]
pub enum LineError {
    /// No output received within the idle timeout.
    IdleTimeout,
    /// I/O error from the underlying reader.
    Io(std::io::Error),
    /// The reader thread exited (EOF or dropped).
    Eof,
}

impl TimedLineReader {
    /// Spawn a reader thread for `source` with the given idle timeout.
    pub fn new<R: BufRead + Send + 'static>(source: R, idle_timeout: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("line-reader".into())
            .spawn(move || {
                let mut reader = source;
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            // Strip trailing newline to match `lines()` behavior.
                            if line.ends_with('\n') {
                                line.pop();
                                if line.ends_with('\r') {
                                    line.pop();
                                }
                            }
                            if tx.send(Ok(line)).is_err() {
                                break; // receiver dropped
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e));
                            break;
                        }
                    }
                }
            })
            .expect("failed to spawn line-reader thread");

        Self { rx, idle_timeout }
    }

    /// Wait for the next line, returning `LineError::IdleTimeout` if no
    /// line arrives within the configured idle timeout.
    pub fn next_line(&self) -> Result<String, LineError> {
        match self.rx.recv_timeout(self.idle_timeout) {
            Ok(Ok(line)) => Ok(line),
            Ok(Err(io_err)) => Err(LineError::Io(io_err)),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(LineError::IdleTimeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(LineError::Eof),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_lines_normally() {
        let data = Cursor::new("line1\nline2\nline3\n");
        let reader = TimedLineReader::new(data, Duration::from_secs(5));

        assert_eq!(reader.next_line().unwrap(), "line1");
        assert_eq!(reader.next_line().unwrap(), "line2");
        assert_eq!(reader.next_line().unwrap(), "line3");
        assert!(matches!(reader.next_line(), Err(LineError::Eof)));
    }

    #[test]
    fn idle_timeout_fires_when_no_data() {
        // A reader that blocks forever (pipe with no writer).
        let (read_end, _write_end) = os_pipe::pipe().unwrap();
        let buf = std::io::BufReader::new(read_end);
        let reader = TimedLineReader::new(buf, Duration::from_millis(100));

        let start = std::time::Instant::now();
        let result = reader.next_line();
        let elapsed = start.elapsed();

        assert!(matches!(result, Err(LineError::IdleTimeout)));
        assert!(elapsed >= Duration::from_millis(90));
        assert!(elapsed < Duration::from_secs(2));
    }

    #[test]
    fn handles_crlf_line_endings() {
        let data = Cursor::new("hello\r\nworld\r\n");
        let reader = TimedLineReader::new(data, Duration::from_secs(5));

        assert_eq!(reader.next_line().unwrap(), "hello");
        assert_eq!(reader.next_line().unwrap(), "world");
        assert!(matches!(reader.next_line(), Err(LineError::Eof)));
    }

    #[test]
    fn handles_empty_lines() {
        let data = Cursor::new("\n\nfoo\n");
        let reader = TimedLineReader::new(data, Duration::from_secs(5));

        assert_eq!(reader.next_line().unwrap(), "");
        assert_eq!(reader.next_line().unwrap(), "");
        assert_eq!(reader.next_line().unwrap(), "foo");
        assert!(matches!(reader.next_line(), Err(LineError::Eof)));
    }
}
