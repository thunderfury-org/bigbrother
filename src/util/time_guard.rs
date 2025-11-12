use std::time::Instant;

use tracing::info;

pub struct TimeGuard {
    label: String,
    start: Instant,
}

impl TimeGuard {
    pub fn new<T: Into<String>>(label: T) -> Self {
        Self {
            label: label.into(),
            start: Instant::now(),
        }
    }
}

impl Drop for TimeGuard {
    fn drop(&mut self) {
        let ms = self.start.elapsed().as_secs_f64() * 1000.0;
        info!("{} took {:.3}ms", self.label, ms);
    }
}

#[macro_export]
macro_rules! log_time {
    ($name:expr) => {
        let _guard = $crate::util::time_guard::TimeGuard::new($name);
    };
}
