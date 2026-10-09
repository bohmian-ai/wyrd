//! Closed failure vocabulary for the node's Bifrost storage owner.

/// One closed storage failure with a scrubbed operator-facing detail.
///
/// Backend failures stay `object_store`'s own typed errors; this enum carries
/// only the configuration refusals the storage owner raises itself.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BifrostStorageError {
    /// The configured policy or storage locator is not usable.
    #[error("bifrost storage configuration is invalid: {detail}")]
    InvalidConfiguration {
        /// Scrubbed description naming the field and the bound it violated.
        detail: String,
    },
}

/// Returns whether an error chain contains an exact filesystem or
/// `object_store` not-found cause.
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
        {
            return true;
        }
        current = source.source();
    }
    false
}
