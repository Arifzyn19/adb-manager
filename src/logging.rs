//! File logging via `tracing` (never spam stdout in production).

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init_logging() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let dir = crate::config::AppConfig::log_dir();
        let _ = std::fs::create_dir_all(&dir);

        let file_appender = tracing_appender::rolling::daily(dir, "adb-manager.log");
        let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);
        // Leak the guard so the background writer lives for the whole process.
        std::mem::forget(_guard);

        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

        let _ = tracing_subscriber::registry()
            .with(fmt::layer().with_writer(file_writer))
            .with(filter)
            .try_init();
    });
}
