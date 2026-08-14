#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportStatus {
    Running,
    Succeeded,
    PartiallyFailed,
    Failed,
    Skipped,
}

impl ImportStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::PartiallyFailed => "partially_failed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    pub(crate) fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "running" => Self::Running,
            "succeeded" => Self::Succeeded,
            "partially_failed" => Self::PartiallyFailed,
            "failed" => Self::Failed,
            "skipped" => Self::Skipped,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips_through_string() {
        for status in [
            ImportStatus::Running,
            ImportStatus::Succeeded,
            ImportStatus::PartiallyFailed,
            ImportStatus::Failed,
            ImportStatus::Skipped,
        ] {
            assert_eq!(ImportStatus::from_str(status.as_str()), Some(status));
        }
    }

    #[test]
    fn unknown_status_string_returns_none() {
        assert_eq!(ImportStatus::from_str("nope"), None);
    }
}
