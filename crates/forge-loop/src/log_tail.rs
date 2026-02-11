use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

pub const DEFAULT_OUTPUT_TAIL_LINES: usize = 60;

pub struct LoopLogger {
    file: File,
}

impl LoopLogger {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self { file })
    }

    pub fn write(&mut self, payload: &[u8]) -> io::Result<usize> {
        let written = self.file.write(payload)?;
        self.file.flush()?;
        Ok(written)
    }

    pub fn write_line(&mut self, message: &str) -> io::Result<()> {
        let stamp = now_rfc3339_utc();
        self.write_line_with_timestamp(&stamp, message)
    }

    pub fn write_line_with_timestamp(
        &mut self,
        timestamp_rfc3339: &str,
        message: &str,
    ) -> io::Result<()> {
        self.file
            .write_all(format!("[{timestamp_rfc3339}] {message}\n").as_bytes())?;
        self.file.flush()?;
        Ok(())
    }
}

pub struct TailWriter {
    max_lines: usize,
    lines: Vec<String>,
    buffer: String,
}

impl TailWriter {
    pub fn new(max_lines: usize) -> Self {
        Self {
            max_lines: if max_lines == 0 {
                DEFAULT_OUTPUT_TAIL_LINES
            } else {
                max_lines
            },
            lines: Vec::new(),
            buffer: String::new(),
        }
    }

    pub fn write(&mut self, payload: &[u8]) -> usize {
        let text = format!("{}{}", self.buffer, String::from_utf8_lossy(payload));
        let parts: Vec<&str> = text.split('\n').collect();
        if parts.is_empty() {
            return payload.len();
        }

        self.buffer = parts[parts.len() - 1].to_string();
        for line in &parts[..parts.len() - 1] {
            if self.lines.len() >= self.max_lines {
                let _ = self.lines.remove(0);
            }
            self.lines.push((*line).to_string());
        }
        payload.len()
    }

    pub fn as_string(&self) -> String {
        let mut out = self.lines.clone();
        if !self.buffer.trim().is_empty() {
            out.push(self.buffer.clone());
        }
        out.join("\n")
    }
}

pub fn tail_file(path: &Path, max_lines: usize) -> io::Result<String> {
    if max_lines == 0 {
        return Ok(String::new());
    }
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut lines: Vec<String> = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if lines.len() >= max_lines {
            let _ = lines.remove(0);
        }
        lines.push(line);
    }
    Ok(lines.join("\n"))
}

fn now_rfc3339_utc() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(value) => value.as_secs() as i64,
        Err(_) => 0,
    };
    // Simple RFC3339-like UTC stamp; tests use write_line_with_timestamp for determinism.
    format!("{now}Z")
}

#[cfg(test)]
mod tests {
    use super::{tail_file, LoopLogger, TailWriter, DEFAULT_OUTPUT_TAIL_LINES};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn loop_logger_appends_and_formats_lines() {
        let path = temp_path("log-tail-writer");
        let mut logger = match LoopLogger::open(path.as_path()) {
            Ok(value) => value,
            Err(err) => panic!("open logger failed: {err}"),
        };
        if let Err(err) = logger.write(b"raw\n") {
            panic!("raw write failed: {err}");
        }
        if let Err(err) = logger.write_line_with_timestamp("2026-02-09T17:00:00Z", "hello") {
            panic!("line write failed: {err}");
        }
        drop(logger);

        let content = match fs::read_to_string(path.as_path()) {
            Ok(value) => value,
            Err(err) => panic!("read logger content failed: {err}"),
        };
        assert_eq!(content, "raw\n[2026-02-09T17:00:00Z] hello\n");
    }

    #[test]
    fn tail_writer_keeps_only_last_n_complete_lines() {
        let mut writer = TailWriter::new(2);
        assert_eq!(writer.write(b"one\ntwo\nthree\n"), 14);
        assert_eq!(writer.as_string(), "two\nthree");
    }

    #[test]
    fn tail_writer_tracks_partial_line_buffer() {
        let mut writer = TailWriter::new(3);
        let _ = writer.write(b"line-1\nline");
        let _ = writer.write(b"-2");
        assert_eq!(writer.as_string(), "line-1\nline-2");
    }

    #[test]
    fn tail_writer_ignores_blank_partial_buffer() {
        let mut writer = TailWriter::new(3);
        let _ = writer.write(b"a\n   ");
        assert_eq!(writer.as_string(), "a");
    }

    #[test]
    fn tail_writer_uses_default_when_max_zero() {
        let mut writer = TailWriter::new(0);
        for idx in 0..(DEFAULT_OUTPUT_TAIL_LINES + 2) {
            let _ = writer.write(format!("line-{idx}\n").as_bytes());
        }
        let out = writer.as_string();
        assert!(out.starts_with("line-2"));
        assert!(out.ends_with(&format!("line-{}", DEFAULT_OUTPUT_TAIL_LINES + 1)));
    }

    #[test]
    fn tail_file_returns_last_n_lines() {
        let path = temp_path("tail-file");
        if let Err(err) = fs::write(path.as_path(), "l1\nl2\nl3\nl4\n") {
            panic!("write fixture failed: {err}");
        }
        let out = match tail_file(path.as_path(), 2) {
            Ok(value) => value,
            Err(err) => panic!("tail file failed: {err}"),
        };
        assert_eq!(out, "l3\nl4");
    }

    #[test]
    fn tail_file_zero_lines_returns_empty() {
        let path = temp_path("tail-empty");
        if let Err(err) = fs::write(path.as_path(), "l1\nl2\n") {
            panic!("write fixture failed: {err}");
        }
        let out = match tail_file(path.as_path(), 0) {
            Ok(value) => value,
            Err(err) => panic!("tail file failed: {err}"),
        };
        assert!(out.is_empty());
    }

    fn temp_path(prefix: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let nonce = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(value) => value.as_nanos(),
            Err(_) => 0,
        };
        path.push(format!("{prefix}-{}-{nonce}.log", std::process::id()));
        path
    }
}
