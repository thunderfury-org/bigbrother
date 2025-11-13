use std::{num::NonZeroU8, panic, thread};

use time::{
    UtcOffset,
    format_description::well_known::{
        self,
        iso8601::{Config, EncodedConfig, TimePrecision},
    },
};
use tracing::{error, level_filters::LevelFilter};
use tracing_appender::rolling::Rotation;
use tracing_subscriber::fmt::time::OffsetTime;

const MAX_LEVEL: LevelFilter = LevelFilter::INFO;
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
        .max_log_files(7)
        .build(log_dir)
        .expect("Failed to initialize rolling file appender");

    let offset = match UtcOffset::current_local_offset() {
        Ok(o) => o,
        Err(_) => UtcOffset::UTC,
    };

    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_timer(OffsetTime::new(offset, well_known::Iso8601::<CONFIG>))
        .with_max_level(MAX_LEVEL)
        .with_ansi(false)
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
