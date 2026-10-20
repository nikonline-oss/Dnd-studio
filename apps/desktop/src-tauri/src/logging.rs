//! Logging (CLAUDE.md §10): tracing → size-rotated files (5 × 10 MB) + an in-memory ring buffer
//! for the log panel, pushed live to the webview.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

/// Size of one log file before rotation.
pub const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;
/// Number of log files kept: `dndstudio.log` plus `dndstudio.1.log` … `dndstudio.4.log`.
pub const MAX_LOG_FILES: usize = 5;
/// Records kept in memory for the log panel.
pub const BUFFER_CAPACITY: usize = 2000;

/// An `io::Write` that rotates by size: when the next write would exceed `max_bytes`, the current
/// file becomes `<stem>.1.<ext>`, older ones shift up and the oldest beyond `max_files` is deleted.
/// (`tracing-appender` only rotates by time, ADR-0010.)
#[derive(Debug)]
pub struct RotatingFileWriter {
    dir: PathBuf,
    stem: String,
    ext: String,
    max_bytes: u64,
    max_files: usize,
    file: File,
    size: u64,
}

impl RotatingFileWriter {
    pub fn new(dir: &Path, file_name: &str, max_bytes: u64, max_files: usize) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let (stem, ext) = file_name.rsplit_once('.').unwrap_or((file_name, "log"));
        let path = dir.join(file_name);
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let size = file.metadata()?.len();
        Ok(Self {
            dir: dir.to_owned(),
            stem: stem.to_owned(),
            ext: ext.to_owned(),
            max_bytes,
            max_files: max_files.max(1),
            file,
            size,
        })
    }

    /// Path of the `n`-th file (`0` = current).
    #[must_use]
    pub fn path(&self, n: usize) -> PathBuf {
        if n == 0 {
            self.dir.join(format!("{}.{}", self.stem, self.ext))
        } else {
            self.dir.join(format!("{}.{n}.{}", self.stem, self.ext))
        }
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file.flush()?;
        let oldest = self.path(self.max_files - 1);
        if oldest.exists() {
            std::fs::remove_file(oldest)?;
        }
        for n in (0..self.max_files - 1).rev() {
            let from = self.path(n);
            if from.exists() {
                std::fs::rename(from, self.path(n + 1))?;
            }
        }
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path(0))?;
        self.size = 0;
        Ok(())
    }
}

impl Write for RotatingFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = buf.len() as u64;
        if self.size > 0 && self.size + len > self.max_bytes {
            self.rotate()?;
        }
        self.file.write_all(buf)?;
        self.size += len;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<Level> for LogLevel {
    fn from(level: Level) -> Self {
        match level {
            Level::TRACE => Self::Trace,
            Level::DEBUG => Self::Debug,
            Level::INFO => Self::Info,
            Level::WARN => Self::Warn,
            Level::ERROR => Self::Error,
        }
    }
}

/// One log line for the panel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct LogRecord {
    /// Unix milliseconds.
    #[specta(type = specta_typescript::Number)]
    pub ts_ms: i64,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
}

type Sink = Box<dyn Fn(&LogRecord) + Send + Sync>;

/// Recent records plus an optional live sink (set once the Tauri app exists).
#[derive(Clone, Default)]
pub struct LogBuffer {
    records: Arc<Mutex<VecDeque<LogRecord>>>,
    sink: Arc<OnceLock<Sink>>,
}

impl std::fmt::Debug for LogBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogBuffer").finish_non_exhaustive()
    }
}

impl LogBuffer {
    #[must_use]
    pub fn recent(&self) -> Vec<LogRecord> {
        self.records
            .lock()
            .map(|r| r.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Forwards every new record to `sink` (e.g. emits a Tauri event). Only the first call wins.
    pub fn set_sink(&self, sink: impl Fn(&LogRecord) + Send + Sync + 'static) {
        let _ = self.sink.set(Box::new(sink));
    }

    fn push(&self, record: LogRecord) {
        if let Ok(mut records) = self.records.lock() {
            if records.len() == BUFFER_CAPACITY {
                records.pop_front();
            }
            records.push_back(record.clone());
        }
        if let Some(sink) = self.sink.get() {
            sink(&record);
        }
    }
}

thread_local! {
    /// Set while forwarding a record, so logging done by the sink itself is not re-forwarded.
    static IN_SINK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// tracing layer that copies events into a [`LogBuffer`].
struct BufferLayer(LogBuffer);

impl<S: Subscriber> Layer<S> for BufferLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if IN_SINK.with(std::cell::Cell::get) {
            return;
        }
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let record = LogRecord {
            ts_ms: crate::local_server::now_ms(),
            level: (*event.metadata().level()).into(),
            target: event.metadata().target().to_owned(),
            message: visitor.finish(),
        };
        IN_SINK.with(|f| f.set(true));
        self.0.push(record);
        IN_SINK.with(|f| f.set(false));
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: String,
    fields: Vec<String>,
}

impl MessageVisitor {
    fn finish(self) -> String {
        if self.fields.is_empty() {
            self.message
        } else {
            format!("{} {}", self.message, self.fields.join(" "))
        }
    }
}

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_owned();
        } else {
            self.fields.push(format!("{}={value}", field.name()));
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.fields.push(format!("{}={value:?}", field.name()));
        }
    }
}

/// Installs the global subscriber: file (rotated), stderr in debug builds, and the panel buffer.
/// Keep the returned guard alive for the whole process, it flushes the file writer on drop.
pub fn init(log_dir: &Path) -> io::Result<(WorkerGuard, LogBuffer)> {
    let writer = RotatingFileWriter::new(log_dir, "dndstudio.log", MAX_LOG_BYTES, MAX_LOG_FILES)?;
    let (file, guard) = tracing_appender::non_blocking(writer);
    let buffer = LogBuffer::default();

    let ours = if cfg!(debug_assertions) {
        Level::DEBUG
    } else {
        Level::INFO
    };
    let filter = Targets::new()
        .with_default(Level::WARN)
        .with_target("dnd_desktop_lib", ours)
        .with_target("dnd_db", ours)
        .with_target("frontend", Level::DEBUG);

    let registry = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file)
                .with_ansi(false),
        )
        .with(BufferLayer(buffer.clone()))
        .with(filter);
    let result = if cfg!(debug_assertions) {
        registry
            .with(tracing_subscriber::fmt::layer().with_writer(io::stderr))
            .try_init()
    } else {
        registry.try_init()
    };
    if result.is_err() {
        eprintln!("a tracing subscriber was already installed");
    }

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(target: "panic", "{info}");
        default_hook(info);
    }));

    Ok((guard, buffer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_by_size_and_keeps_max_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = RotatingFileWriter::new(dir.path(), "app.log", 100, 3).unwrap();
        for i in 0..20 {
            w.write_all(format!("line {i:03} ............................\n").as_bytes())
                .unwrap();
        }
        w.flush().unwrap();
        let mut names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["app.1.log", "app.2.log", "app.log"]);
        for name in &names {
            let len = std::fs::metadata(dir.path().join(name)).unwrap().len();
            assert!(len <= 100, "{name} is {len} bytes");
        }
        // The newest line is in the current file, older ones were rotated out in order.
        let current = std::fs::read_to_string(dir.path().join("app.log")).unwrap();
        assert!(current.contains("line 019"));
        let older = std::fs::read_to_string(dir.path().join("app.2.log")).unwrap();
        assert!(!older.contains("line 019"));
    }

    #[test]
    fn appends_to_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("app.log"), "old\n").unwrap();
        let mut w = RotatingFileWriter::new(dir.path(), "app.log", 1000, 2).unwrap();
        w.write_all(b"new\n").unwrap();
        w.flush().unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("app.log")).unwrap(),
            "old\nnew\n"
        );
    }

    #[test]
    fn buffer_is_bounded_and_forwards() {
        let buffer = LogBuffer::default();
        let seen = Arc::new(Mutex::new(0usize));
        let counter = seen.clone();
        buffer.set_sink(move |_| *counter.lock().unwrap() += 1);
        for i in 0..(BUFFER_CAPACITY + 10) {
            buffer.push(LogRecord {
                ts_ms: i as i64,
                level: LogLevel::Info,
                target: "t".into(),
                message: format!("{i}"),
            });
        }
        let recent = buffer.recent();
        assert_eq!(recent.len(), BUFFER_CAPACITY);
        assert_eq!(recent[0].message, "10");
        assert_eq!(*seen.lock().unwrap(), BUFFER_CAPACITY + 10);
    }
}
