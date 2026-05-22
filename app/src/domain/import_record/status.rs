use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportStatus {
    Pending,
    Running,
    Succeeded,
    PartiallyFailed,
    Failed,
    Skipped,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusTransitionError {
    pub from: ImportStatus,
    pub to: ImportStatus,
}

impl fmt::Display for StatusTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "illegal import status transition: {:?} -> {:?}",
            self.from, self.to
        )
    }
}

impl ImportStatus {
    pub(crate) fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::PartiallyFailed | Self::Failed | Self::Skipped
        )
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::PartiallyFailed => "partially_failed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    pub(crate) fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "pending" => Self::Pending,
            "running" => Self::Running,
            "succeeded" => Self::Succeeded,
            "partially_failed" => Self::PartiallyFailed,
            "failed" => Self::Failed,
            "skipped" => Self::Skipped,
            _ => return None,
        })
    }

    #[allow(dead_code)]
    pub(crate) fn transition_to(
        self,
        next: ImportStatus,
    ) -> Result<ImportStatus, StatusTransitionError> {
        let allowed = match (self, next) {
            (Self::Pending, Self::Running) => true,
            (Self::Pending, t) if t.is_terminal() => true,
            (Self::Running, t) if t.is_terminal() => true,
            _ => false,
        };
        if allowed {
            Ok(next)
        } else {
            Err(StatusTransitionError {
                from: self,
                to: next,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_transitions_to_running() {
        assert_eq!(
            ImportStatus::Pending.transition_to(ImportStatus::Running),
            Ok(ImportStatus::Running)
        );
    }

    #[test]
    fn pending_may_skip_running_and_go_to_terminal_when_no_work_is_executed() {
        assert_eq!(
            ImportStatus::Pending.transition_to(ImportStatus::Skipped),
            Ok(ImportStatus::Skipped)
        );
    }

    #[test]
    fn running_transitions_to_each_terminal_status() {
        for terminal in [
            ImportStatus::Succeeded,
            ImportStatus::PartiallyFailed,
            ImportStatus::Failed,
            ImportStatus::Skipped,
        ] {
            assert_eq!(
                ImportStatus::Running.transition_to(terminal),
                Ok(terminal),
                "running -> {:?} should be allowed",
                terminal
            );
        }
    }

    #[test]
    fn terminal_statuses_cannot_transition_further() {
        for terminal in [
            ImportStatus::Succeeded,
            ImportStatus::PartiallyFailed,
            ImportStatus::Failed,
            ImportStatus::Skipped,
        ] {
            let err = terminal.transition_to(ImportStatus::Running).unwrap_err();
            assert_eq!(err.from, terminal);
            assert_eq!(err.to, ImportStatus::Running);
        }
    }

    #[test]
    fn running_cannot_go_back_to_pending() {
        let err = ImportStatus::Running
            .transition_to(ImportStatus::Pending)
            .unwrap_err();
        assert_eq!(err.from, ImportStatus::Running);
        assert_eq!(err.to, ImportStatus::Pending);
    }

    #[test]
    fn status_round_trips_through_string() {
        for status in [
            ImportStatus::Pending,
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
