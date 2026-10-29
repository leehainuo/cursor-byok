//! Captures startup diagnostics before the database and desktop runtime initialize.
use std::{error::Error, path::PathBuf};

use rfd::{MessageButtons, MessageDialog, MessageLevel};
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

const LOG_DIRECTORY_NAME: &str = "logs";
const LOG_FILE_PREFIX: &str = "cursor-byok";
const LOG_FILE_SUFFIX: &str = "log";
const RETAINED_LOG_FILES: usize = 15;

type BoxError = Box<dyn Error + Send + Sync>;

pub(crate) struct StartupDiagnostics {
    log_directory: PathBuf,
    _writer_guard: WorkerGuard,
}

impl StartupDiagnostics {
    pub(crate) fn initialize() -> Result<Self, BoxError> {
        let log_directory = cursor_server::config::managed_data_dir()?.join(LOG_DIRECTORY_NAME);
        std::fs::create_dir_all(&log_directory)?;

        let file_appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix(LOG_FILE_PREFIX)
            .filename_suffix(LOG_FILE_SUFFIX)
            .max_log_files(RETAINED_LOG_FILES)
            .build(&log_directory)?;
        let (file_writer, writer_guard) = tracing_appender::non_blocking(file_appender);
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "cursor_byok_desktop=info,cursor_server=info".into());

        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(file_writer),
            )
            .try_init()?;

        Ok(Self {
            log_directory,
            _writer_guard: writer_guard,
        })
    }

    pub(crate) fn log_directory(&self) -> &std::path::Path {
        &self.log_directory
    }

    pub(crate) fn report_fatal(&self, error: &(dyn Error + 'static)) {
        let details = error_chain(error);
        tracing::error!(
            error = %details,
            log_directory = %self.log_directory.display(),
            "desktop failed to start"
        );
        show_fatal_dialog(&details, Some(&self.log_directory));
    }
}

pub(crate) fn report_logging_failure(error: &(dyn Error + 'static)) {
    let details = error_chain(error);
    eprintln!("Cursor BYOK failed to initialize logging: {details}");
    show_fatal_dialog(&details, None);
}

fn show_fatal_dialog(details: &str, log_directory: Option<&std::path::Path>) {
    let log_guidance = match log_directory {
        Some(directory) => format!(
            "日志目录 / Log directory:\n{}\n\n请将最新的日志文件发送给开发者。\nPlease send the latest log file to the developer.",
            directory.display()
        ),
        None => "日志系统也未能启动，因此没有生成日志文件。\nLogging also failed to initialize, so no log file was created."
            .to_owned(),
    };
    let description = format!(
        "Cursor BYOK 无法启动 / failed to start.\n\n错误 / Error:\n{details}\n\n{log_guidance}"
    );

    let _ = MessageDialog::new()
        .set_level(MessageLevel::Error)
        .set_title("Cursor BYOK 启动失败 / Startup Error")
        .set_description(description)
        .set_buttons(MessageButtons::Ok)
        .show();
}

fn error_chain(error: &(dyn Error + 'static)) -> String {
    let mut details = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        details.push_str("\nCaused by: ");
        details.push_str(&cause.to_string());
        source = cause.source();
    }
    details
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt;

    #[derive(Debug)]
    struct OuterError(std::io::Error);

    impl fmt::Display for OuterError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("database initialization failed")
        }
    }

    impl Error for OuterError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(&self.0)
        }
    }

    #[test]
    fn fatal_report_includes_the_complete_error_chain() {
        let error = OuterError(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "database file is read-only",
        ));

        assert_eq!(
            error_chain(&error),
            "database initialization failed\nCaused by: database file is read-only"
        );
    }
}
