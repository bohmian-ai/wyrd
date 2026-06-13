//! PromptCard envelope validation.

use std::collections::HashSet;

use serde_json::json;
use thiserror::Error;

use crate::card::prompt::{
    ParameterName, PromptSpec, extract_media_placeholders, extract_text_placeholders,
};
use crate::error::WyrdError;

/// PromptCard validation and boundary-mapping failures.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PromptError {
    /// A declared variable name did not match the prompt parameter grammar.
    #[error("invalid variable name: {name}")]
    InvalidVariableName {
        /// Offending variable name.
        name: String,
    },
    /// A variable name appeared more than once in `prompt.variables`.
    #[error("duplicate variable name: {name}")]
    DuplicateVariable {
        /// Duplicated variable name.
        name: String,
    },
    /// A `{{name}}` placeholder was present but not declared.
    #[error("undeclared placeholder: {name}")]
    UndeclaredPlaceholder {
        /// Placeholder name that was not declared.
        name: String,
    },
    /// A declared variable was not referenced in the native request.
    #[error("unreferenced variable: {name}")]
    UnreferencedVariable {
        /// Variable name that was never referenced.
        name: String,
    },
    /// A `${media:name}` placeholder was present but not declared.
    #[error("undeclared media placeholder: {name}")]
    UndeclaredMediaPlaceholder {
        /// Placeholder name that was not declared.
        name: String,
    },
    /// A declared media variable was not referenced in the native request.
    #[error("unreferenced media variable: {name}")]
    UnreferencedMediaVariable {
        /// Media variable name that was never referenced.
        name: String,
    },
    /// A media placeholder was not isolated in its own text part.
    #[error("media placeholder is not isolated: {name}")]
    MediaPlaceholderNotIsolated {
        /// Placeholder name.
        name: String,
    },
    /// A media placeholder appeared in system content.
    #[error("media placeholder in system message: {name}")]
    MediaInSystemMessage {
        /// Placeholder name.
        name: String,
    },
    /// Provider rejected a media kind/source combination.
    #[error("unsupported media kind {kind} for provider {provider}")]
    UnsupportedMediaForProvider {
        /// Provider name.
        provider: String,
        /// Media kind.
        kind: String,
    },
    /// Media type or URI was invalid.
    #[error("invalid media type: {message}")]
    InvalidMediaType {
        /// Validation message.
        message: String,
    },
    /// Render was attempted with an unbound media variable.
    #[error("missing media variable: {name}")]
    MissingMediaVariable {
        /// Missing media variable name.
        name: String,
    },
    /// Media path was not a regular file.
    #[error("media path is not a regular file: {path}")]
    MediaNotRegularFile {
        /// Local path.
        path: String,
    },
    /// Media path exceeded the byte limit.
    #[error("media file too large: {path}")]
    MediaTooLarge {
        /// Local path.
        path: String,
        /// Observed size.
        size: u64,
        /// Maximum size.
        limit: u64,
    },
    /// Media path extension was not recognized.
    #[error("invalid media extension for {kind}: {path}")]
    MediaInvalidExtension {
        /// Local path.
        path: String,
        /// Media kind.
        kind: String,
    },
    /// Media IO failed.
    #[error("media io error: {message}")]
    MediaIo {
        /// IO message.
        message: String,
    },
    /// The native prompt model string was empty.
    #[error("prompt model must be non-empty")]
    EmptyModel,
    /// The prompt response JSON Schema was not a JSON object.
    #[error("response_type JsonSchema schema must be a JSON object")]
    InvalidResponseSchema,
    /// Loader extension was not supported by pure codec helpers.
    #[error("prompt card loader extension is unsupported: {extension:?}")]
    LoaderBadExtension {
        /// Unsupported extension value.
        extension: Option<String>,
    },
    /// Filesystem loader IO failure, raised by downstream IO-owning crates.
    #[error("prompt card loader IO failure at {path}: {message}")]
    LoaderIo {
        /// Path string supplied by the IO-owning caller.
        path: String,
        /// Source IO message.
        message: String,
    },
    /// Native request serialization failed during placeholder validation.
    #[error("failed to serialize native prompt request: {message}")]
    SerializeRequest {
        /// Serialization failure message.
        message: String,
    },
    /// Provider authentication failed at a downstream boundary.
    #[error("{provider} provider authentication failed: {detail}")]
    ProviderAuth {
        /// Provider name.
        provider: String,
        /// Redacted auth failure detail.
        detail: String,
    },
    /// Provider rate-limited a downstream request.
    #[error("{provider} provider rate limit")]
    ProviderRateLimit {
        /// Provider name.
        provider: String,
        /// Retry hint in milliseconds when provided.
        retry_after_ms: Option<u64>,
    },
    /// Provider returned an upstream failure.
    #[error("{provider} provider upstream failure {status}: {detail}")]
    ProviderUpstream {
        /// Provider name.
        provider: String,
        /// Upstream status code.
        status: u16,
        /// Redacted upstream body or detail.
        detail: String,
    },
    /// Provider request exceeded its deadline.
    #[error("{provider} provider request timed out")]
    ProviderTimeout {
        /// Provider name.
        provider: String,
    },
    /// Request variant did not match the dialed provider.
    #[error("request variant {sent} does not match provider {provider}")]
    ProviderMismatch {
        /// Sent request variant name.
        sent: String,
        /// Dialed provider name.
        provider: String,
    },
    /// A typed model_settings object belonged to a different provider.
    #[error("model_settings provider mismatch: expected {expected}, got {got}")]
    SettingsProviderMismatch {
        /// Expected provider.
        expected: String,
        /// Supplied settings provider.
        got: String,
    },
    /// A model_settings value could not decode into native settings.
    #[error("model_settings for {provider} failed to decode: {message}")]
    SettingsDecode {
        /// Provider being decoded.
        provider: String,
        /// Decode failure message.
        message: String,
    },
    /// Render-time variable was missing.
    #[error("missing render variable: {name}")]
    MissingVariable {
        /// Missing variable name.
        name: String,
    },
    /// Message handoff conversion is unsupported.
    #[error("unsupported handoff from {src} to {dst}")]
    UnsupportedHandoff {
        /// Source provider name.
        src: String,
        /// Destination provider name.
        dst: String,
    },
    /// Runtime response schema validation failed.
    #[error("response did not match schema {schema_name}: {message}")]
    ResponseDecode {
        /// Response schema name.
        schema_name: String,
        /// Decode or validation message.
        message: String,
    },
    /// A declarative `PromptDraft` failed to compile into a native prompt.
    #[error("prompt draft compile failed for provider {provider}: {message}")]
    DraftInvalid {
        /// Provider name from the draft.
        provider: String,
        /// Compile failure message.
        message: String,
    },
}

impl PromptError {
    /// Returns the stable `WYRD_PROMPT_*` code for this prompt error.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidVariableName { .. } => "WYRD_PROMPT_400_INVALID_VARIABLE_NAME",
            Self::DuplicateVariable { .. } => "WYRD_PROMPT_409_DUPLICATE_VARIABLE",
            Self::UndeclaredPlaceholder { .. } => "WYRD_PROMPT_422_UNDECLARED_PLACEHOLDER",
            Self::UnreferencedVariable { .. } => "WYRD_PROMPT_422_UNREFERENCED_VARIABLE",
            Self::UndeclaredMediaPlaceholder { .. } => {
                "WYRD_PROMPT_422_UNDECLARED_MEDIA_PLACEHOLDER"
            }
            Self::UnreferencedMediaVariable { .. } => "WYRD_PROMPT_422_UNREFERENCED_MEDIA_VARIABLE",
            Self::MediaPlaceholderNotIsolated { .. } => {
                "WYRD_PROMPT_422_MEDIA_PLACEHOLDER_NOT_ISOLATED"
            }
            Self::MediaInSystemMessage { .. } => "WYRD_PROMPT_400_MEDIA_IN_SYSTEM_MESSAGE",
            Self::UnsupportedMediaForProvider { .. } => {
                "WYRD_PROMPT_400_UNSUPPORTED_MEDIA_FOR_PROVIDER"
            }
            Self::InvalidMediaType { .. } => "WYRD_PROMPT_400_INVALID_MEDIA_TYPE",
            Self::MissingMediaVariable { .. } => "WYRD_PROMPT_422_MISSING_MEDIA_VARIABLE",
            Self::MediaNotRegularFile { .. } => "WYRD_PROMPT_400_MEDIA_NOT_REGULAR_FILE",
            Self::MediaTooLarge { .. } => "WYRD_PROMPT_400_MEDIA_TOO_LARGE",
            Self::MediaInvalidExtension { .. } => "WYRD_PROMPT_400_MEDIA_INVALID_EXTENSION",
            Self::MediaIo { .. } => "WYRD_PROMPT_500_MEDIA_IO",
            Self::EmptyModel => "WYRD_PROMPT_400_EMPTY_MODEL",
            Self::InvalidResponseSchema => "WYRD_PROMPT_400_INVALID_RESPONSE_SCHEMA",
            Self::LoaderBadExtension { .. } => "WYRD_PROMPT_400_LOADER_BAD_EXTENSION",
            Self::LoaderIo { .. } => "WYRD_PROMPT_500_LOADER_IO",
            Self::SerializeRequest { .. } => "WYRD_PROMPT_500_SERIALIZE_REQUEST",
            Self::ProviderAuth { .. } => "WYRD_PROMPT_401_PROVIDER_AUTH",
            Self::ProviderRateLimit { .. } => "WYRD_PROMPT_429_PROVIDER_RATE_LIMIT",
            Self::ProviderUpstream { .. } => "WYRD_PROMPT_502_PROVIDER_UPSTREAM",
            Self::ProviderTimeout { .. } => "WYRD_PROMPT_504_PROVIDER_TIMEOUT",
            Self::ProviderMismatch { .. } => "WYRD_PROMPT_400_PROVIDER_MISMATCH",
            Self::SettingsProviderMismatch { .. } => "WYRD_PROMPT_400_SETTINGS_PROVIDER_MISMATCH",
            Self::SettingsDecode { .. } => "WYRD_PROMPT_400_SETTINGS_DECODE",
            Self::MissingVariable { .. } => "WYRD_PROMPT_422_MISSING_VARIABLE",
            Self::UnsupportedHandoff { .. } => "WYRD_PROMPT_501_UNSUPPORTED_HANDOFF",
            Self::ResponseDecode { .. } => "WYRD_PROMPT_422_RESPONSE_DECODE",
            Self::DraftInvalid { .. } => "WYRD_PROMPT_400_DRAFT_INVALID",
        }
    }
}

/// Runs the six PromptCard envelope invariants in deterministic order.
pub fn validate(spec: &PromptSpec) -> Result<(), WyrdError> {
    if spec.prompt.model.trim().is_empty() {
        return Err(PromptError::EmptyModel.into());
    }

    let mut variable_seen = HashSet::new();
    for name in &spec.prompt.variables {
        ParameterName::new(name.clone())?;
        if !variable_seen.insert(name.as_str()) {
            return Err(PromptError::DuplicateVariable { name: name.clone() }.into());
        }
    }

    let mut media_seen = HashSet::new();
    for name in &spec.prompt.media_variables {
        ParameterName::new(name.clone())?;
        if !media_seen.insert(name.as_str()) {
            return Err(PromptError::DuplicateVariable { name: name.clone() }.into());
        }
    }

    let referenced = extract_text_placeholders(spec)?;
    let declared: HashSet<&str> = spec.prompt.variables.iter().map(String::as_str).collect();
    let referenced_set: HashSet<&str> = referenced.iter().map(String::as_str).collect();

    if let Some(name) = referenced_set.difference(&declared).next() {
        return Err(PromptError::UndeclaredPlaceholder {
            name: (*name).to_owned(),
        }
        .into());
    }

    let media_referenced = extract_media_placeholders(spec)?;
    let media_declared: HashSet<&str> = spec
        .prompt
        .media_variables
        .iter()
        .map(String::as_str)
        .collect();
    let media_referenced_set: HashSet<&str> = media_referenced.iter().map(String::as_str).collect();

    if let Some(name) = media_referenced_set.difference(&media_declared).next() {
        return Err(PromptError::UndeclaredMediaPlaceholder {
            name: (*name).to_owned(),
        }
        .into());
    }

    if let Some(name) = media_declared.difference(&media_referenced_set).next() {
        return Err(PromptError::UnreferencedMediaVariable {
            name: (*name).to_owned(),
        }
        .into());
    }

    if let Some(name) = declared.difference(&referenced_set).next() {
        return Err(PromptError::UnreferencedVariable {
            name: (*name).to_owned(),
        }
        .into());
    }

    if let skald_spec::ResponseType::JsonSchema { schema, .. } = &spec.prompt.response_type
        && !schema.is_object()
    {
        return Err(PromptError::InvalidResponseSchema.into());
    }

    Ok(())
}

impl From<PromptError> for WyrdError {
    fn from(error: PromptError) -> Self {
        let message = error.to_string();
        match error {
            PromptError::InvalidVariableName { name } => WyrdError::PromptInvalidVariableName {
                message,
                details: json!({ "name": name }),
            },
            PromptError::DuplicateVariable { name } => WyrdError::PromptDuplicateVariable {
                message,
                details: json!({ "name": name }),
            },
            PromptError::UndeclaredPlaceholder { name } => WyrdError::PromptUndeclaredPlaceholder {
                message,
                details: json!({ "name": name }),
            },
            PromptError::UnreferencedVariable { name } => WyrdError::PromptUnreferencedVariable {
                message,
                details: json!({ "name": name }),
            },
            PromptError::UndeclaredMediaPlaceholder { name } => {
                WyrdError::PromptUndeclaredMediaPlaceholder {
                    message,
                    details: json!({ "name": name }),
                }
            }
            PromptError::UnreferencedMediaVariable { name } => {
                WyrdError::PromptUnreferencedMediaVariable {
                    message,
                    details: json!({ "name": name }),
                }
            }
            PromptError::MediaPlaceholderNotIsolated { name } => {
                WyrdError::PromptMediaPlaceholderNotIsolated {
                    message,
                    details: json!({ "name": name }),
                }
            }
            PromptError::MediaInSystemMessage { name } => WyrdError::PromptMediaInSystemMessage {
                message,
                details: json!({ "name": name }),
            },
            PromptError::UnsupportedMediaForProvider { provider, kind } => {
                WyrdError::PromptUnsupportedMediaForProvider {
                    message,
                    details: json!({ "provider": provider, "kind": kind }),
                }
            }
            PromptError::InvalidMediaType { message: source } => {
                WyrdError::PromptInvalidMediaType {
                    message,
                    details: json!({ "source": source }),
                }
            }
            PromptError::MissingMediaVariable { name } => WyrdError::PromptMissingMediaVariable {
                message,
                details: json!({ "name": name }),
            },
            PromptError::MediaNotRegularFile { path } => WyrdError::PromptMediaNotRegularFile {
                message,
                details: json!({ "path": path }),
            },
            PromptError::MediaTooLarge { path, size, limit } => WyrdError::PromptMediaTooLarge {
                message,
                details: json!({ "path": path, "size": size, "limit": limit }),
            },
            PromptError::MediaInvalidExtension { path, kind } => {
                WyrdError::PromptMediaInvalidExtension {
                    message,
                    details: json!({ "path": path, "kind": kind }),
                }
            }
            PromptError::MediaIo { message: source } => WyrdError::PromptMediaIo {
                message,
                details: json!({ "source": source }),
            },
            PromptError::EmptyModel => WyrdError::PromptEmptyModel {
                message,
                details: json!({}),
            },
            PromptError::InvalidResponseSchema => WyrdError::PromptInvalidResponseSchema {
                message,
                details: json!({}),
            },
            PromptError::LoaderBadExtension { extension } => WyrdError::PromptLoaderBadExtension {
                message,
                details: json!({ "extension": extension }),
            },
            PromptError::LoaderIo {
                path,
                message: source,
            } => WyrdError::PromptLoaderIo {
                message,
                details: json!({ "path": path, "source": source }),
            },
            PromptError::SerializeRequest { message: source } => {
                WyrdError::PromptSerializeRequest {
                    message,
                    details: json!({ "source": source }),
                }
            }
            PromptError::ProviderAuth { provider, detail } => WyrdError::PromptProviderAuth {
                message,
                details: json!({ "provider": provider, "detail": detail }),
            },
            PromptError::ProviderRateLimit {
                provider,
                retry_after_ms,
            } => WyrdError::PromptProviderRateLimit {
                message,
                details: json!({ "provider": provider, "retry_after_ms": retry_after_ms }),
            },
            PromptError::ProviderUpstream {
                provider,
                status,
                detail,
            } => WyrdError::PromptProviderUpstream {
                message,
                details: json!({ "provider": provider, "status": status, "detail": detail }),
            },
            PromptError::ProviderTimeout { provider } => WyrdError::PromptProviderTimeout {
                message,
                details: json!({ "provider": provider }),
            },
            PromptError::ProviderMismatch { sent, provider } => WyrdError::PromptProviderMismatch {
                message,
                details: json!({ "sent": sent, "provider": provider }),
            },
            PromptError::SettingsProviderMismatch { expected, got } => {
                WyrdError::PromptSettingsProviderMismatch {
                    message,
                    details: json!({ "expected": expected, "got": got }),
                }
            }
            PromptError::SettingsDecode {
                provider,
                message: source,
            } => WyrdError::PromptSettingsDecode {
                message,
                details: json!({ "provider": provider, "source": source }),
            },
            PromptError::MissingVariable { name } => WyrdError::PromptMissingVariable {
                message,
                details: json!({ "name": name }),
            },
            PromptError::UnsupportedHandoff { src, dst } => WyrdError::PromptUnsupportedHandoff {
                message,
                details: json!({ "src": src, "dst": dst }),
            },
            PromptError::ResponseDecode {
                schema_name,
                message: source,
            } => WyrdError::PromptResponseDecode {
                message,
                details: json!({ "schema_name": schema_name, "source": source }),
            },
            PromptError::DraftInvalid {
                provider,
                message: source,
            } => WyrdError::PromptDraftInvalid {
                message,
                details: json!({ "provider": provider, "source": source }),
            },
        }
    }
}
