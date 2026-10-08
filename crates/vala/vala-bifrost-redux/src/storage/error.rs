//! Closed failure vocabulary for the node's Bifrost storage owner.

/// One closed storage failure with a scrubbed operator-facing detail.
///
/// `object_store` classifies backend failures; this enum carries the classes
/// Bifrost callers act on. A missing object is kept distinct because Oracle
/// reports a vanished pinned object as stale rather than as an outage.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BifrostStorageError {
    /// The object does not exist at the validated location.
    #[error("bifrost storage object not found: {detail}")]
    NotFound {
        /// Scrubbed description with no path, tenant, or credential content.
        detail: String,
    },
    /// The backend refused the request for authorization reasons.
    #[error("bifrost storage permission denied: {detail}")]
    PermissionDenied {
        /// Scrubbed description with no path, tenant, or credential content.
        detail: String,
    },
    /// Any other backend failure that does not fit a narrower class.
    #[error("bifrost storage backend failure: {detail}")]
    Backend {
        /// Scrubbed description with no path, tenant, or credential content.
        detail: String,
    },
    /// The configured policy or storage locator is not usable.
    #[error("bifrost storage configuration is invalid: {detail}")]
    InvalidConfiguration {
        /// Scrubbed description naming the field and the bound it violated.
        detail: String,
    },
}

impl BifrostStorageError {
    /// Maps one `object_store` failure into this closed vocabulary.
    ///
    /// The detail is a fixed phrase per class rather than the error's message,
    /// because `object_store` messages embed the object path.
    #[must_use]
    pub fn from_object_store(error: &object_store::Error) -> Self {
        match error {
            object_store::Error::NotFound { .. } => Self::NotFound {
                detail: "the object does not exist".to_owned(),
            },
            object_store::Error::PermissionDenied { .. }
            | object_store::Error::Unauthenticated { .. } => Self::PermissionDenied {
                detail: "the backend refused the request".to_owned(),
            },
            object_store::Error::UnknownConfigurationKey { .. } => Self::InvalidConfiguration {
                detail: "the backend configuration is not usable".to_owned(),
            },
            _ => Self::Backend {
                detail: "the backend request failed".to_owned(),
            },
        }
    }
}

/// Returns whether an error chain contains an exact filesystem,
/// `object_store`, or storage-owner not-found cause.
///
/// Object-store backends report a vanished object through their own typed
/// error, wrapped an arbitrary number of times by Iceberg, Parquet, and
/// `DataFusion`. Walking the whole chain and downcasting is the only way to
/// separate a pinned object that disappeared after cut selection from a
/// genuine storage outage, which the caller must classify differently.
pub(crate) fn error_chain_contains_not_found(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut current = Some(error);
    while let Some(source) = current {
        if source
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            || matches!(
                source.downcast_ref::<object_store::Error>(),
                Some(object_store::Error::NotFound { .. })
            )
            || matches!(
                source.downcast_ref::<BifrostStorageError>(),
                Some(BifrostStorageError::NotFound { .. })
            )
        {
            return true;
        }
        current = source.source();
    }
    false
}
