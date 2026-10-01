//! Slack `chat.postMessage` wire format.
//!
//! Owns only the Slack request body and the interpretation of Slack's JSON
//! reply. Screening, credential attachment, response bounds, and HTTP status
//! classification stay with [`OperatorDelivery`](super::OperatorDelivery).

use serde_json::Value;
use wyrd_spec::card::operator::OperatorFailureContext;

use super::{Attempt, INVALID_REQUEST, PROVIDER_REJECTED, PROVIDER_TRANSIENT};

/// Slack errors that are transient rather than configuration failures.
const TRANSIENT: [&str; 5] = [
    "ratelimited",
    "internal_error",
    "service_unavailable",
    "request_timeout",
    "fatal_error",
];

/// The `chat.postMessage` body posting the rendered `text` to `channel_id`.
///
/// # Errors
/// Returns a terminal [`INVALID_REQUEST`] attempt when the text template does
/// not render against `context`.
pub(super) fn message(
    channel_id: &str,
    text: &str,
    context: &OperatorFailureContext,
) -> Result<Value, Attempt> {
    let text = context
        .render(text)
        .map_err(|_| Attempt::terminal(INVALID_REQUEST, "the Slack text template is invalid"))?;
    Ok(serde_json::json!({ "channel": channel_id, "text": text }))
}

/// Classify a 2xx Slack reply body: delivered when its JSON `ok` is true,
/// otherwise a retry for a Slack-declared transient error and a terminal
/// rejection for any other (or missing) error code.
pub(super) fn outcome(reply: &[u8]) -> Attempt {
    let reply: Value = serde_json::from_slice(reply).unwrap_or_default();
    if reply["ok"] == Value::Bool(true) {
        return Attempt::Delivered;
    }
    let code = reply["error"].as_str().unwrap_or("unknown_error");
    let message = format!("Slack refused the message: {code}");
    if TRANSIENT.contains(&code) {
        Attempt::retry(PROVIDER_TRANSIENT, &message)
    } else {
        Attempt::terminal(PROVIDER_REJECTED, &message)
    }
}

#[cfg(test)]
mod tests {
    //! Slack reply classification.

    use super::*;

    /// `ok: true` delivers, a declared transient error retries, and any other
    /// or unparseable reply is a terminal rejection.
    #[test]
    fn slack_reply_classification() {
        assert_eq!(outcome(br#"{"ok":true}"#), Attempt::Delivered);
        assert!(matches!(
            outcome(br#"{"ok":false,"error":"ratelimited"}"#),
            Attempt::Retry { .. }
        ));
        for reply in [
            &br#"{"ok":false,"error":"channel_not_found"}"#[..],
            b"not json",
        ] {
            assert!(matches!(outcome(reply), Attempt::Terminal(_)));
        }
    }
}
