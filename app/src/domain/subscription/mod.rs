#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubscriptionMediaType {
    Movie,
    Tv,
}

impl SubscriptionMediaType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Tv => "tv",
        }
    }

    pub(crate) fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "movie" => Self::Movie,
            "tv" => Self::Tv,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_type_round_trips_through_string() {
        for mt in [SubscriptionMediaType::Movie, SubscriptionMediaType::Tv] {
            assert_eq!(SubscriptionMediaType::from_str(mt.as_str()), Some(mt));
        }
    }

    #[test]
    fn unknown_media_type_string_returns_none() {
        assert_eq!(SubscriptionMediaType::from_str("nope"), None);
    }
}
