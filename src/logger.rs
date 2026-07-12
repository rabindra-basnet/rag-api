//! Structured logging setup.
//!
//! Logs are split by concern into separate files under `logs/`:
//!   - `app.log`         — application/server logs (the `rag_backend` crate, including errors)
//!   - `database.log`    — SQLx query logs (DB access)
//!   - `web.log`         — HTTP layer logs (`tower_http`: access/request traces + statuses)
//!
//! The active file for each is named `<prefix>.log`. At the end of each day it is
//! rotated: gzipped to `<prefix>.<YYYY-MM-DD>.log.gz` and a fresh `<prefix>.log` is
//! started. Everything is also mirrored to the console. The non-blocking writer
//! guards are kept alive in a `OnceLock` for the lifetime of the process.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use chrono::Utc;
use flate2::write::GzEncoder;
use flate2::Compression;
use tracing_subscriber::fmt::writer::MakeWriter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::prelude::*;

static GUARDS: OnceLock<Vec<WorkerGuard>> = OnceLock::new();

fn today() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

struct RollingInner {
    prefix: String,
    file: io::BufWriter<std::fs::File>,
    date: String,
}

/// Writes to `<prefix>.log`; rotates (gzip + new file) when the calendar day changes.
struct RollingWriter {
    inner: Arc<Mutex<RollingInner>>,
}

impl RollingWriter {
    fn new(prefix: &str) -> Self {
        let date = today();
        let path = format!("logs/{prefix}.log");
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .expect("failed to open log file");
        RollingWriter {
            inner: Arc::new(Mutex::new(RollingInner {
                prefix: prefix.to_string(),
                file: io::BufWriter::new(file),
                date,
            })),
        }
    }
}

impl RollingInner {
    fn maybe_rotate(&mut self) {
        let current = today();
        if current == self.date {
            return;
        }
        let closed = self.date.clone();
        // Flush and close the current file.
        let _ = self.file.flush();
        let placeholder = OpenOptions::new()
            .read(true)
            .open(format!("logs/{}.log", self.prefix))
            .expect("log placeholder");
        let mut old = std::mem::replace(&mut self.file, io::BufWriter::new(placeholder));
        let _ = old.flush();
        drop(old);

        // Gzip the rotated day into <prefix>.<closed>.log.gz and remove the plaintext.
        let src = format!("logs/{}.log", self.prefix);
        let dst = format!("logs/{}.{}.log.gz", self.prefix, closed);
        if let Ok(data) = std::fs::read(&src) {
            if let Ok(gz) = std::fs::File::create(&dst) {
                let mut enc = GzEncoder::new(gz, Compression::default());
                let _ = enc.write_all(&data);
                let _ = enc.finish();
            }
            let _ = std::fs::remove_file(&src);
        }

        // Start a fresh active file.
        if let Ok(nf) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&src)
        {
            self.file = io::BufWriter::new(nf);
        }
        self.date = current;
    }
}

impl Write for RollingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut g = self.inner.lock().unwrap();
        g.maybe_rotate();
        g.file.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.lock().unwrap().file.flush()
    }
}

impl MakeWriter<'_> for RollingWriter {
    type Writer = RollingWriter;
    fn make_writer(&self) -> RollingWriter {
        RollingWriter {
            inner: self.inner.clone(),
        }
    }
}

fn writer(prefix: &str) -> RollingWriter {
    RollingWriter::new(prefix)
}

pub fn init(dev: bool) {
    let default_filter = if dev {
        "rag_backend=trace,tower_http=debug,sqlx=debug"
    } else {
        "rag_backend=info,tower_http=info"
    };
    let base_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| default_filter.into());

    let logs_dir = Path::new("logs");
    if !logs_dir.exists() {
        std::fs::create_dir_all(logs_dir).ok();
    }

    let (app_nb, app_guard) = tracing_appender::non_blocking(writer("app"));
    let (db_nb, db_guard) = tracing_appender::non_blocking(writer("database"));
    let (web_nb, web_guard) = tracing_appender::non_blocking(writer("web"));

    let console_layer = tracing_subscriber::fmt::layer()
        .with_file(dev)
        .with_line_number(dev)
        .with_filter(base_filter);

    let app_layer = tracing_subscriber::fmt::layer()
        .with_writer(app_nb)
        .with_ansi(false)
        .with_target(true)
        .with_filter(EnvFilter::new("rag_backend=trace"));

    let db_layer = tracing_subscriber::fmt::layer()
        .with_writer(db_nb)
        .with_ansi(false)
        .with_target(true)
        .with_filter(EnvFilter::new("sqlx=debug"));

    let web_layer = tracing_subscriber::fmt::layer()
        .with_writer(web_nb)
        .with_ansi(false)
        .with_target(true)
        .with_filter(EnvFilter::new("tower_http=debug"));

    tracing_subscriber::registry()
        .with(app_layer)
        .with(db_layer)
        .with(web_layer)
        .with(console_layer)
        .init();

    // Keep the background writer workers alive for the whole process.
    let _ = GUARDS.set(vec![app_guard, db_guard, web_guard]);
}
