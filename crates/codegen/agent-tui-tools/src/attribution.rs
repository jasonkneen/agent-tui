//! 401 attribution: callback hook + shared helpers for tool HTTP clients.

use std::sync::Arc;

use agent_tui_auth::bearer_suffix;

pub use agent_tui_auth::bearer_fragment::BEARER_SUFFIX_LEN;

/// Which tool endpoint produced the 401.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolConsumer {
    ImageGen,
    VideoGenStart,
    VideoGenPoll,
    WebSearch,
}

impl ToolConsumer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ImageGen => "ImageGen",
            Self::VideoGenStart => "VideoGen.start",
            Self::VideoGenPoll => "VideoGen.poll",
            Self::WebSearch => "WebSearch",
        }
    }
}

/// 401 attribution callback. Shell wires this to emit telemetry.
pub trait Auth401AttributionCallback: Send + Sync + std::fmt::Debug {
    /// `sent_bearer_suffix` is truncated to [`BEARER_SUFFIX_LEN`]
    /// before crossing this boundary. `None` = no bearer was sent.
    fn record_401(&self, consumer: ToolConsumer, sent_bearer_suffix: Option<&str>);
}

/// Shared, cheap-to-clone alias for the attribution callback.
pub type SharedAttributionCallback = Arc<dyn Auth401AttributionCallback>;

/// Record a 401 if a callback is wired, truncating to the tail first so only
/// the fragment is ever materialized.
pub(crate) fn emit_401(
    callback: Option<&SharedAttributionCallback>,
    consumer: ToolConsumer,
    sent_bearer: Option<&str>,
) {
    if let Some(cb) = callback {
        let suffix = sent_bearer.map(|s| bearer_suffix(s).to_string());
        cb.record_401(consumer, suffix.as_deref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_to_prefix_long_string_cuts_at_12() {
        assert_eq!(
            truncate_to_prefix("xai-key-aaaaaaaaaaaaaaaaaaa".to_string()),
            "xai-key-aaaa"
        );
    }

    #[test]
    fn truncate_to_prefix_short_string_unchanged() {
        assert_eq!(truncate_to_prefix("abc".to_string()), "abc");
    }

    #[test]
    fn truncate_to_prefix_exact_12_unchanged() {
        assert_eq!(
            truncate_to_prefix("123456789012".to_string()),
            "123456789012"
        );
        assert_eq!(truncate_to_prefix("123456789012".to_string()).len(), 12);
    }

    #[test]
    fn truncate_to_prefix_empty_unchanged() {
        assert_eq!(truncate_to_prefix(String::new()), "");
    }

    #[test]
    fn tool_consumer_as_str_stable_identifiers() {
        assert_eq!(ToolConsumer::ImageGen.as_str(), "ImageGen");
        assert_eq!(ToolConsumer::VideoGenStart.as_str(), "VideoGen.start");
        assert_eq!(ToolConsumer::VideoGenPoll.as_str(), "VideoGen.poll");
        assert_eq!(ToolConsumer::WebSearch.as_str(), "WebSearch");
    }
}
