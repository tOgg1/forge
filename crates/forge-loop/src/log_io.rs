use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;

pub const DEFAULT_OUTPUT_TAIL_LINES: usize = 60;

pub struct LoopLogger {
    writer: Mutex<BufWriter<File>>,
}

impl LoopLogger {
    pub fn new(path: &Path) -> Result<Self, String> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|err| err.to_string())?;
        Ok(Self {
            writer: Mutex::new(BufWriter::new(file)),
        })
    }

    pub fn write_line(&self, message: &str) -> Result<(), String> {
        let stamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let mut writer = self.writer.lock().map_err(|err| err.to_string())?;
        writer
            .write_all(format!("[{stamp}] {message}\n").as_bytes())
            .map_err(|err| err.to_string())?;
        writer.flush().map_err(|err| err.to_string())
    }
}

impl Write for LoopLogger {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| std::io::Error::other("loop logger mutex poisoned"))?;
        let n = writer.write(buf)?;
        writer.flush()?;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| std::io::Error::other("loop logger mutex poisoned"))?;
        writer.flush()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

    pub fn tail_string(&self) -> String {
        let mut lines = self.lines.clone();
        if !self.buffer.trim().is_empty() {
            lines.push(self.buffer.clone());
        }
        lines.join("\n")
    }
}

impl Write for TailWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let text = format!("{}{}", self.buffer, String::from_utf8_lossy(buf));
        let mut parts = text.split('\n').map(|s| s.to_string()).collect::<Vec<_>>();
        if parts.is_empty() {
            return Ok(buf.len());
        }

        self.buffer = parts.pop().unwrap_or_default();
        for line in parts {
            if self.lines.len() >= self.max_lines {
                self.lines.remove(0);
            }
            self.lines.push(line);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{LoopLogger, TailWriter, DEFAULT_OUTPUT_TAIL_LINES};
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn loop_logger_write_line_prefixes_timestamp_and_message() {
        let temp = TempDir::new("forge-loop-log");
        let path = temp.path().join("loop.log");
        let logger = match LoopLogger::new(&path) {
            Ok(logger) => logger,
            Err(err) => panic!("new logger failed: {err}"),
        };

        if let Err(err) = logger.write_line("loop started") {
            panic!("write_line failed: {err}");
        }

        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => panic!("read log failed: {err}"),
        };
        assert!(text.contains("] loop started\n"));
        assert!(text.starts_with('['));
    }

    #[test]
    fn loop_logger_write_flushes_immediately() {
        let temp = TempDir::new("forge-loop-log-write");
        let path = temp.path().join("loop.log");
        let mut logger = match LoopLogger::new(&path) {
            Ok(logger) => logger,
            Err(err) => panic!("new logger failed: {err}"),
        };

        if let Err(err) = logger.write_all(b"hello\n") {
            panic!("write_all failed: {err}");
        }

        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => panic!("read log failed: {err}"),
        };
        assert_eq!(text, "hello\n");
    }

    #[test]
    fn tail_writer_keeps_only_last_n_lines() {
        let mut writer = TailWriter::new(2);
        if let Err(err) = writer.write_all(b"a\nb\nc\n") {
            panic!("write failed: {err}");
        }
        assert_eq!(writer.tail_string(), "b\nc");
    }

    #[test]
    fn tail_writer_keeps_partial_line_until_completed() {
        let mut writer = TailWriter::new(10);
        if let Err(err) = writer.write_all(b"first\npart") {
            panic!("write failed: {err}");
        }
        assert_eq!(writer.tail_string(), "first\npart");

        if let Err(err) = writer.write_all(b"ial\nsecond\n") {
            panic!("write failed: {err}");
        }
        assert_eq!(writer.tail_string(), "first\npartial\nsecond");
    }

    #[test]
    fn tail_writer_ignores_whitespace_only_buffer() {
        let mut writer = TailWriter::new(10);
        if let Err(err) = writer.write_all(b"a\n   ") {
            panic!("write failed: {err}");
        }
        assert_eq!(writer.tail_string(), "a");
    }

    #[test]
    fn tail_writer_zero_max_uses_default() {
        let mut writer = TailWriter::new(0);
        for idx in 0..(DEFAULT_OUTPUT_TAIL_LINES + 5) {
            let line = format!("l{idx}\n");
            if let Err(err) = writer.write_all(line.as_bytes()) {
                panic!("write failed: {err}");
            }
        }
        let output = writer.tail_string();
        assert!(output.contains(&format!("l{}", DEFAULT_OUTPUT_TAIL_LINES + 4)));
        assert!(!output.contains("l0\n"));
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!(
                "{prefix}-{}-{}",
                std::process::id(),
                monotonic_nanos()
            ));
            if let Err(err) = fs::create_dir_all(&path) {
                panic!("create_dir_all failed {}: {err}", path.display());
            }
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn monotonic_nanos() -> u128 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(_) => 0,
        }
    }
}
