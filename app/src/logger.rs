use std::{num::NonZeroU8, panic, thread};

use time::{
    UtcOffset,
    format_description::well_known::{
        self,
        iso8601::{Config, EncodedConfig, TimePrecision},
    },
};
use tracing::error;
use tracing_appender::rolling::Rotation;
use tracing_subscriber::{
    EnvFilter, Layer,
    filter::FilterFn,
    fmt::{self, time::OffsetTime},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

const CONFIG: EncodedConfig = Config::DEFAULT
    .set_time_precision(TimePrecision::Second {
        decimal_digits: NonZeroU8::new(3),
    })
    .encode();

pub fn init(log_dir: &str) {
    let writer = tracing_appender::rolling::Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("bigbrother")
        .filename_suffix("log")
        .max_log_files(3)
        .build(log_dir)
        .expect("Failed to initialize rolling file appender");

    let access_log_writer = tracing_appender::rolling::Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("access.http")
        .filename_suffix("log")
        .max_log_files(3)
        .build(log_dir)
        .expect("Failed to initialize access log appender");

    let offset = match UtcOffset::current_local_offset() {
        Ok(o) => o,
        Err(_) => UtcOffset::UTC,
    };

    // Main application logger
    let app_logger = fmt::layer()
        .with_writer(writer)
        .with_timer(OffsetTime::new(offset, well_known::Iso8601::<CONFIG>))
        .with_ansi(false)
        .with_filter(FilterFn::new(|meta| {
            if *meta.level() > tracing::Level::INFO {
                return false;
            }

            // Exclude access log entries
            !(meta.is_event() && meta.target() == "bigbrother::interface::http::log")
        }));

    // Separate logger for access.log
    let access_logger = fmt::layer()
        .with_writer(access_log_writer)
        .with_timer(OffsetTime::new(offset, well_known::Iso8601::<CONFIG>))
        .with_ansi(false)
        .with_filter(EnvFilter::new("off,bigbrother::interface::http::log=info"));

    tracing_subscriber::registry()
        .with(access_logger)
        .with(app_logger)
        .init();

    log_panic();
}

fn log_panic() {
    // catch panic and log them using tracing instead of default output to StdErr
    panic::set_hook(Box::new(|info| {
        let thread = thread::current();
        let thread = thread.name().unwrap_or("unknown");

        let msg = match info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => match info.payload().downcast_ref::<String>() {
                Some(s) => &**s,
                None => "Box<Any>",
            },
        };

        let backtrace = backtrace::Backtrace::new();

        match info.location() {
            Some(location) => {
                // without backtrace
                if msg.starts_with("notrace - ") {
                    error!(
                        target: "panic", "thread '{}' panicked at '{}': {}:{}",
                        thread,
                        msg.replace("notrace - ", ""),
                        location.file(),
                        location.line()
                    );
                }
                // with backtrace
                else {
                    error!(
                        target: "panic", "thread '{}' panicked at '{}': {}:{}\n{:?}",
                        thread,
                        msg,
                        location.file(),
                        location.line(),
                        backtrace
                    );
                }
            }
            None => {
                // without backtrace
                if msg.starts_with("notrace - ") {
                    error!(
                        target: "panic", "thread '{}' panicked at '{}'",
                        thread,
                        msg.replace("notrace - ", ""),
                    );
                }
                // with backtrace
                else {
                    error!(
                        target: "panic", "thread '{}' panicked at '{}'\n{:?}",
                        thread,
                        msg,
                        backtrace
                    );
                }
            }
        }
    }));
}
