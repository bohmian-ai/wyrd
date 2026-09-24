//! Typed side-effect templates dispatched after a failed verification.
//!
//! An Operator names its credential authority by a tenant-scoped connection
//! name, never a secret or an environment variable. Its text, URL, header, and
//! JSON-body templates may reference only the fields of the bounded
//! [`OperatorFailureContext`] through `{{field}}` placeholders; registration
//! rejects any other field through [`OperatorSpec::validate`].

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::WyrdError;
use crate::ids::{
    BindingId, CardUid, ConnectionName, OperatorDispatchId, VerificationResultId, VerificationRunId,
};
use crate::operator_connection::{
    HttpAuthScheme, HttpsOrigin, OperatorConnectionConfig, OperatorProvider,
};
use crate::reference::Ref;
use crate::verification::VerificationVerdict;

/// Server ceiling on one external delivery attempt, in seconds.
///
/// An authored `timeout_seconds` is clipped to this value; a Card cannot raise it.
pub const MAX_ATTEMPT_SECONDS: u32 = 30;

/// Maximum length, in characters, of the failure-context result summary.
pub const MAX_SUMMARY_CHARS: usize = 512;

/// Authored request headers the server owns and an Operator may never set.
///
/// Compared case-insensitively. `Idempotency-Key` carries the dispatch ID and
/// the rest are transport or credential headers.
pub const FORBIDDEN_HTTP_HEADERS: [&str; 6] = [
    "authorization",
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "idempotency-key",
];

/// A server-owned side-effect template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct OperatorSpec {
    /// Optional human-readable purpose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The single action performed when this operator fires.
    ///
    /// Flattened so that an inline `on_failure` mapping and a referenced
    /// Operator Card's `spec` share one wire shape.
    #[serde(flatten)]
    pub action: OperatorAction,
    /// Optional execution limits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<OperatorBudget>,
}

impl OperatorSpec {
    /// Validate the connection-independent shape of this Operator.
    ///
    /// Checks every `{{field}}` template against the closed failure-context
    /// field set, requires a literal `https` (or loopback `http`) URL origin
    /// with templates confined to its path and query, refuses server-owned
    /// headers, and refuses a custom auth header that is itself forbidden.
    /// Connection existence and authority are checked by the server, which
    /// owns the tenant's connections.
    ///
    /// # Errors
    /// Returns [`WyrdError::SpecInvalidOperator`] naming `field` and the first
    /// violation found.
    pub fn validate(&self, field: &str) -> Result<(), WyrdError> {
        let invalid = |at: &str, reason: String| WyrdError::SpecInvalidOperator {
            message: format!("{field}{at}: {reason}"),
            details: serde_json::json!({ "field": format!("{field}{at}"), "reason": reason }),
        };
        let template = |at: &str, value: &str| {
            render_template(value, |_| Some(String::new())).map_err(|reason| invalid(at, reason))
        };
        match &self.action {
            OperatorAction::Workflow { .. } => Ok(()),
            OperatorAction::Notify {
                channel:
                    NotifyChannel::Slack {
                        channel_id, text, ..
                    },
            } => {
                if channel_id.trim().is_empty() {
                    return Err(invalid(
                        ".channel.channel_id",
                        "must not be empty".to_owned(),
                    ));
                }
                template(".channel.text", text).map(drop)
            }
            OperatorAction::Notify {
                channel:
                    NotifyChannel::PagerDuty {
                        route,
                        summary,
                        dedup_key,
                        ..
                    },
            } => {
                if route.trim().is_empty() {
                    return Err(invalid(".channel.route", "must not be empty".to_owned()));
                }
                template(".channel.summary", summary)?;
                dedup_key
                    .as_deref()
                    .map_or(Ok(()), |key| template(".channel.dedup_key", key).map(drop))
            }
            OperatorAction::Http {
                url,
                headers,
                body,
                auth,
                expect_status,
                ..
            } => {
                template(".url", url)?;
                operator_url_origin(url).map_err(|reason| invalid(".url", reason))?;
                for (name, value) in headers {
                    let at = format!(".headers.{name}");
                    check_header_name(name).map_err(|reason| invalid(&at, reason))?;
                    template(&at, value)?;
                }
                if let Some(HttpAuth::Header { name, .. }) = auth {
                    check_header_name(name).map_err(|reason| invalid(".auth.name", reason))?;
                    if headers.keys().any(|h| h.eq_ignore_ascii_case(name)) {
                        return Err(invalid(
                            ".auth.name",
                            format!("header {name} is also authored in headers"),
                        ));
                    }
                }
                if let Some(body) = body {
                    check_body_templates(body).map_err(|reason| invalid(".body", reason))?;
                }
                match expect_status {
                    Some(status) if !(100..=599).contains(status) => Err(invalid(
                        ".expect_status",
                        format!("{status} is not an HTTP status"),
                    )),
                    _ => Ok(()),
                }
            }
        }
    }
}

impl OperatorSpec {
    /// The provider and name of the connection this Operator's credential
    /// authority names, or `None` for Workflow and unauthenticated HTTP.
    #[must_use]
    pub fn connection(&self) -> Option<(OperatorProvider, &ConnectionName)> {
        match &self.action {
            OperatorAction::Notify {
                channel: NotifyChannel::Slack { connection, .. },
            } => Some((OperatorProvider::Slack, connection)),
            OperatorAction::Notify {
                channel: NotifyChannel::PagerDuty { connection, .. },
            } => Some((OperatorProvider::PagerDuty, connection)),
            OperatorAction::Http {
                auth: Some(auth), ..
            } => Some((OperatorProvider::Http, auth.connection())),
            OperatorAction::Http { auth: None, .. } | OperatorAction::Workflow { .. } => None,
        }
    }

    /// Whether the stored connection authority `config` authorizes this
    /// Operator.
    ///
    /// The one compatibility predicate registration and every delivery
    /// attempt share. Slack and PagerDuty need only the matching provider; an
    /// authenticated HTTP Operator additionally needs its literal URL origin
    /// to equal the connection origin and its auth scheme (and custom header
    /// name, case-insensitively) to equal the stored scheme.
    #[must_use]
    pub fn matches_authority(&self, config: &OperatorConnectionConfig) -> bool {
        match (&self.action, config) {
            (
                OperatorAction::Notify {
                    channel: NotifyChannel::Slack { .. },
                },
                OperatorConnectionConfig::Slack { .. },
            )
            | (
                OperatorAction::Notify {
                    channel: NotifyChannel::PagerDuty { .. },
                },
                OperatorConnectionConfig::PagerDuty {},
            ) => true,
            (
                OperatorAction::Http {
                    url,
                    auth: Some(auth),
                    ..
                },
                OperatorConnectionConfig::Http {
                    origin,
                    auth: scheme,
                },
            ) => {
                let same_scheme = match (auth, scheme) {
                    (HttpAuth::Bearer { .. }, HttpAuthScheme::Bearer)
                    | (HttpAuth::Basic { .. }, HttpAuthScheme::Basic) => true,
                    (HttpAuth::Header { name, .. }, HttpAuthScheme::Header { name: stored }) => {
                        name.eq_ignore_ascii_case(stored)
                    }
                    _ => false,
                };
                same_scheme && operator_url_origin(url).is_ok_and(|url| url == *origin)
            }
            _ => false,
        }
    }
}

/// The closed set of actions an Operator can perform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum OperatorAction {
    /// Dispatch a registered Workflow.
    Workflow {
        /// Workflow to dispatch with the trigger context.
        workflow_ref: Ref,
    },
    /// Send a typed notification.
    Notify {
        /// Notification destination and payload.
        channel: NotifyChannel,
    },
    /// Perform an HTTP request.
    Http {
        /// HTTP method.
        method: HttpMethod,
        /// URL template; its origin is literal and `{{field}}` placeholders may
        /// appear only in the path and query.
        url: String,
        /// Optional request headers; values are templates.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
        /// Optional structured JSON body; string values are templates.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<Value>,
        /// Credential authority; omitted for an unauthenticated request.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auth: Option<HttpAuth>,
        /// Optional request timeout, clipped to [`MAX_ATTEMPT_SECONDS`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_seconds: Option<u32>,
        /// Optional expected response status; any 2xx is accepted when omitted.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect_status: Option<u16>,
    },
}

/// Typed notification destinations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NotifyChannel {
    /// PagerDuty Events API v2 trigger through a tenant Global Integration key.
    PagerDuty {
        /// Name of the tenant's `pager_duty` connection.
        connection: ConnectionName,
        /// Route sent as `payload.custom_details.wyrd_route` for PagerDuty
        /// Service Routes.
        route: String,
        /// PagerDuty event severity.
        severity: PagerDutySeverity,
        /// Event summary template.
        summary: String,
        /// Optional de-duplication key template; the dispatch ID when omitted.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dedup_key: Option<String>,
    },
    /// Slack `chat.postMessage` through a tenant workspace bot token.
    Slack {
        /// Name of the tenant's `slack` connection.
        connection: ConnectionName,
        /// Slack conversation ID the app may post in.
        channel_id: String,
        /// Message text template.
        text: String,
    },
}

/// PagerDuty Events API v2 severities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PagerDutySeverity {
    /// critical.
    Critical,
    /// error.
    Error,
    /// warning.
    Warning,
    /// info.
    Info,
}

/// HTTP methods supported by an HTTP Operator action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum HttpMethod {
    /// GET.
    Get,
    /// POST.
    Post,
    /// PUT.
    Put,
    /// PATCH.
    Patch,
    /// DELETE.
    Delete,
}

/// Credential authority of an HTTP action, bound to a tenant `http` connection.
///
/// The scheme and custom header name must equal the named connection's stored
/// authority at registration and before every attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "scheme", rename_all = "snake_case", deny_unknown_fields)]
pub enum HttpAuth {
    /// `Authorization: Bearer` from the connection's token.
    Bearer {
        /// Name of the tenant's `http` connection.
        connection: ConnectionName,
    },
    /// `Authorization: Basic` from the connection's username and password.
    Basic {
        /// Name of the tenant's `http` connection.
        connection: ConnectionName,
    },
    /// A custom header carrying the connection's value.
    Header {
        /// Header name, compared case-insensitively with the connection's.
        name: String,
        /// Name of the tenant's `http` connection.
        connection: ConnectionName,
    },
}

impl HttpAuth {
    /// The connection this auth binding names.
    #[must_use]
    pub const fn connection(&self) -> &ConnectionName {
        match self {
            Self::Bearer { connection }
            | Self::Basic { connection }
            | Self::Header { connection, .. } => connection,
        }
    }
}

/// Operator execution limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct OperatorBudget {
    /// Maximum wall-clock runtime in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_wall_seconds: Option<u32>,
    /// Maximum tool calls allowed during invocation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<u32>,
}

/// The bounded, immutable context every Operator invocation renders from.
///
/// Frozen on the dispatch row when a failed binding-created run settles, so
/// every retry renders the same payload. It carries identities, the verdict,
/// completion time, and a bounded summary only: no Eval context, media, Drift
/// feature rows, result detail, or secret material. Template placeholder names
/// are exactly [`OperatorFailureContext::FIELDS`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct OperatorFailureContext {
    /// The dispatch being delivered.
    pub dispatch_id: OperatorDispatchId,
    /// The failed run.
    pub run_id: VerificationRunId,
    /// The run's canonical result.
    pub result_id: VerificationResultId,
    /// The binding that configured this Operator.
    pub binding_id: BindingId,
    /// Exact Verifier Card UID.
    pub verifier_uid: CardUid,
    /// Exact Verifier identity as `space/name@version`.
    pub verifier_ref: String,
    /// Exact subject Card UID.
    pub subject_uid: CardUid,
    /// Exact subject identity as `space/name@version`.
    pub subject_ref: String,
    /// Always `failed`.
    pub verdict: VerificationVerdict,
    /// When the run settled.
    pub completed_at: DateTime<Utc>,
    /// Bounded human-readable result summary.
    pub summary: String,
}

impl OperatorFailureContext {
    /// Every placeholder name a template may reference.
    pub const FIELDS: [&'static str; 11] = [
        "dispatch_id",
        "run_id",
        "result_id",
        "binding_id",
        "verifier_uid",
        "verifier_ref",
        "subject_uid",
        "subject_ref",
        "verdict",
        "completed_at",
        "summary",
    ];

    /// The rendered value of one placeholder, or `None` for an unknown name.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<String> {
        Some(match name {
            "dispatch_id" => self.dispatch_id.to_string(),
            "run_id" => self.run_id.to_string(),
            "result_id" => self.result_id.to_string(),
            "binding_id" => self.binding_id.to_string(),
            "verifier_uid" => self.verifier_uid.to_string(),
            "verifier_ref" => self.verifier_ref.clone(),
            "subject_uid" => self.subject_uid.to_string(),
            "subject_ref" => self.subject_ref.clone(),
            "verdict" => <&'static str>::from(self.verdict).to_owned(),
            "completed_at" => self.completed_at.to_rfc3339(),
            "summary" => self.summary.clone(),
            _ => return None,
        })
    }

    /// Render `template` with this context's fields.
    ///
    /// # Errors
    /// Returns the template error for a malformed or unknown placeholder.
    pub fn render(&self, template: &str) -> Result<String, String> {
        render_template(template, |name| self.field(name))
    }
}

/// Render `{{field}}` placeholders in `template` through `lookup`.
///
/// The grammar is closed: `{{`, optional spaces, one known field name,
/// optional spaces, `}}`. Validation and delivery share this one parser;
/// validation passes a lookup that accepts every known field.
///
/// # Errors
/// Returns a reason for an unterminated placeholder or an unknown field.
pub fn render_template(
    template: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<String, String> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after
            .find("}}")
            .ok_or_else(|| "unterminated {{ placeholder".to_owned())?;
        let name = after[..end].trim();
        if !OperatorFailureContext::FIELDS.contains(&name) {
            return Err(format!(
                "unknown template field {name:?}; allowed: {}",
                OperatorFailureContext::FIELDS.join(", ")
            ));
        }
        out.push_str(&lookup(name).ok_or_else(|| format!("unknown template field {name:?}"))?);
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The literal origin of an HTTP Operator URL template.
///
/// The origin (scheme, host, port) precedes any placeholder, so registration
/// can compare it with a connection's stored authority before any rendering.
///
/// # Errors
/// Returns a reason when a placeholder appears in the origin or the origin is
/// not a valid [`HttpsOrigin`].
pub fn operator_url_origin(url: &str) -> Result<HttpsOrigin, String> {
    let authority_start = url.find("://").map_or(0, |i| i + 3);
    let origin_end = url[authority_start..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |i| authority_start + i);
    let origin = &url[..origin_end];
    if origin.contains("{{") {
        return Err("templates may appear only in the URL path and query".to_owned());
    }
    HttpsOrigin::parse(origin).map_err(|error| error.to_string())
}

/// Refuse an empty, malformed, or server-owned header name.
fn check_header_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b));
    if !valid {
        return Err(format!("{name:?} is not a valid header name"));
    }
    if FORBIDDEN_HTTP_HEADERS
        .iter()
        .any(|forbidden| name.eq_ignore_ascii_case(forbidden))
    {
        return Err(format!("header {name} is owned by the server"));
    }
    Ok(())
}

/// Validate every string template inside a JSON body.
fn check_body_templates(value: &Value) -> Result<(), String> {
    match value {
        Value::String(text) => render_template(text, |_| Some(String::new())).map(drop),
        Value::Array(items) => items.iter().try_for_each(check_body_templates),
        Value::Object(map) => map.values().try_for_each(check_body_templates),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    //! Pure validation of Operator templates, origins, and headers.

    use super::*;

    /// Parse one Operator spec from JSON.
    ///
    /// # Panics
    /// Panics when the fixture is not a valid Operator spec.
    fn spec(value: Value) -> OperatorSpec {
        serde_json::from_value(value).expect("test_setup: operator spec parses")
    }

    /// Build an HTTP Operator with `url`, `headers`, and `auth`.
    fn http(url: &str, headers: Value, auth: Value) -> OperatorSpec {
        let mut value = serde_json::json!({
            "kind": "http", "method": "post", "url": url, "headers": headers,
            "body": { "text": "{{ summary }}", "ids": ["{{run_id}}"] }
        });
        if !auth.is_null() {
            value["auth"] = auth;
        }
        spec(value)
    }

    /// Known fields render; unknown and unterminated placeholders are refused.
    #[test]
    fn templates_accept_only_the_closed_context_fields() {
        let rendered = render_template("run {{run_id}} / {{ summary }}", |name| {
            Some(name.to_uppercase())
        })
        .expect("known fields render");
        assert_eq!(rendered, "run RUN_ID / SUMMARY");
        assert!(render_template("{{secret}}", |_| Some(String::new())).is_err());
        assert!(render_template("{{run_id", |_| Some(String::new())).is_err());
    }

    /// Slack, PagerDuty, and HTTP Operators validate; each violation is refused.
    #[test]
    fn validate_refuses_each_shape_violation() {
        let slack = spec(serde_json::json!({
            "kind": "notify",
            "channel": { "kind": "slack", "connection": "ops-slack", "channel_id": "C1", "text": "{{verifier_ref}} failed" }
        }));
        assert!(slack.validate("spec").is_ok());
        let bad_slack = spec(serde_json::json!({
            "kind": "notify",
            "channel": { "kind": "slack", "connection": "ops-slack", "channel_id": "C1", "text": "{{env.TOKEN}}" }
        }));
        assert_eq!(
            bad_slack
                .validate("spec")
                .expect_err("unknown field")
                .code(),
            "WYRD_SPEC_400_INVALID_OPERATOR"
        );
        let pager = spec(serde_json::json!({
            "kind": "notify",
            "channel": { "kind": "pager_duty", "connection": "pagerduty", "route": "team-a", "severity": "error", "summary": "{{summary}}" }
        }));
        assert!(pager.validate("spec").is_ok());

        let auth =
            serde_json::json!({ "scheme": "header", "name": "X-Api-Key", "connection": "hooks" });
        assert!(
            http(
                "https://hooks.example.com/v1/{{run_id}}",
                serde_json::json!({}),
                auth.clone()
            )
            .validate("spec")
            .is_ok()
        );
        for (url, headers, auth) in [
            (
                "https://{{run_id}}.example.com/",
                serde_json::json!({}),
                Value::Null,
            ),
            (
                "ftp://hooks.example.com/",
                serde_json::json!({}),
                Value::Null,
            ),
            (
                "http://hooks.example.com/",
                serde_json::json!({}),
                Value::Null,
            ),
            (
                "https://hooks.example.com/",
                serde_json::json!({ "AUTHORIZATION": "x" }),
                Value::Null,
            ),
            (
                "https://hooks.example.com/",
                serde_json::json!({ "Idempotency-Key": "x" }),
                Value::Null,
            ),
            (
                "https://hooks.example.com/",
                serde_json::json!({ "x-api-key": "x" }),
                auth.clone(),
            ),
            (
                "https://hooks.example.com/",
                serde_json::json!({}),
                serde_json::json!({ "scheme": "header", "name": "Host", "connection": "hooks" }),
            ),
        ] {
            assert!(http(url, headers, auth).validate("spec").is_err(), "{url}");
        }
    }

    /// The URL origin is extracted literally before any placeholder.
    #[test]
    fn url_origin_is_literal_and_normalized() {
        let origin =
            operator_url_origin("https://Hooks.Example.com:443/a/{{run_id}}?q={{summary}}")
                .expect("literal origin");
        assert_eq!(origin.as_str(), "https://hooks.example.com");
        assert!(operator_url_origin("https://user@hooks.example.com/").is_err());
    }

    /// Authority matches only the same provider, and for HTTP only the same
    /// normalized origin and auth scheme, with header names compared
    /// case-insensitively.
    #[test]
    fn authority_matches_provider_origin_and_scheme() {
        let header =
            serde_json::json!({ "scheme": "header", "name": "x-api-key", "connection": "hooks" });
        let op = http(
            "https://Hooks.Example.com:443/v1/{{run_id}}",
            serde_json::json!({}),
            header,
        );
        assert_eq!(
            op.connection().map(|(p, n)| (p, n.as_str())),
            Some((OperatorProvider::Http, "hooks"))
        );
        let stored = |origin: &str, auth: HttpAuthScheme| OperatorConnectionConfig::Http {
            origin: HttpsOrigin::parse(origin).expect("origin"),
            auth,
        };
        let header_scheme = || HttpAuthScheme::Header {
            name: "X-Api-Key".to_owned(),
        };
        assert!(op.matches_authority(&stored("https://hooks.example.com", header_scheme())));
        assert!(!op.matches_authority(&stored("https://hooks.example.com:8443", header_scheme())));
        assert!(!op.matches_authority(&stored("https://other.example.com", header_scheme())));
        assert!(
            !op.matches_authority(&stored("https://hooks.example.com", HttpAuthScheme::Bearer))
        );
        assert!(!op.matches_authority(&OperatorConnectionConfig::PagerDuty {}));
        let open = http(
            "https://hooks.example.com/x",
            serde_json::json!({}),
            Value::Null,
        );
        assert!(open.connection().is_none());
    }
}
