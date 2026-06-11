//! Server-internal webhook configuration consumed by the alert router.
//!
//! The signer that consumes `WebhookConfig` lives in
//! `crate::alert_router::signer` (PR4.3). This module ships only the
//! typed configuration — no IO, no signing crypto.
