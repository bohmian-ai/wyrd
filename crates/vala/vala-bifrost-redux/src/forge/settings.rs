//! Per-table Forge maintenance settings read from Iceberg table properties.
//!
//! The settings mirror RisingWave's Iceberg sink options and defaults
//! (`connector/src/sink/iceberg/config.rs` at e23ddf95): compaction is off
//! until a table enables it, the interval is one hour, the snapshot-count
//! trigger is disabled, the physical type is `full`, snapshot expiration is on
//! and manifest rewriting is off. They are read by whoever already holds the
//! loaded table, never by the leader while it decides.

use std::collections::HashMap;
use std::time::Duration;

use super::error::ForgeError;

/// Enables leader-scheduled compaction for one table.
pub const ENABLE_COMPACTION_PROPERTY: &str = "wyrd.forge.enable-compaction";
/// Maximum seconds between compactions while commits are pending.
pub const COMPACTION_INTERVAL_PROPERTY: &str = "wyrd.forge.compaction-interval-sec";
/// Pending Iceberg commit count that makes a table due before its interval.
pub const TRIGGER_SNAPSHOT_COUNT_PROPERTY: &str = "wyrd.forge.compaction.trigger-snapshot-count";
/// Physical compaction type a worker plans with.
pub const COMPACTION_TYPE_PROPERTY: &str = "wyrd.forge.compaction.type";
/// Enables leader-timer snapshot expiration for one table.
pub const ENABLE_SNAPSHOT_EXPIRATION_PROPERTY: &str = "wyrd.forge.enable-snapshot-expiration";
/// Enables leader-timer data-manifest rewriting for one table.
pub const ENABLE_MANIFEST_REWRITE_PROPERTY: &str = "wyrd.forge.enable-manifest-rewrite";

/// Physical selection a worker applies to one dispatched compaction task.
///
/// The four values are RisingWave's `CompactionType`; `Full` is its default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ForgeCompactionType {
    /// Choose a delete-heavy or small-file plan from table-wide thresholds.
    Auto,
    /// Rewrite every live data file.
    #[default]
    Full,
    /// Rewrite only data files below the small-file threshold.
    SmallFiles,
    /// Rewrite only data files with associated delete files.
    FilesWithDelete,
}

impl ForgeCompactionType {
    /// Returns the property and wire spelling of this type.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Full => "full",
            Self::SmallFiles => "small-files",
            Self::FilesWithDelete => "files-with-delete",
        }
    }

    /// Parses the property and wire spelling of a type.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] for an unknown spelling.
    pub fn parse(raw: &str) -> Result<Self, ForgeError> {
        match raw {
            "auto" => Ok(Self::Auto),
            "full" => Ok(Self::Full),
            "small-files" => Ok(Self::SmallFiles),
            "files-with-delete" => Ok(Self::FilesWithDelete),
            other => Err(ForgeError::InvalidConfig {
                detail: format!("unknown Forge compaction type {other:?}"),
            }),
        }
    }
}

/// The scheduling and maintenance settings one table declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeTableSettings {
    /// Whether ordinary commit-driven compaction is scheduled.
    pub compaction_enabled: bool,
    /// Longest wait between compactions while at least one commit is pending.
    pub compaction_interval: Duration,
    /// Pending commit count that triggers compaction early.
    pub trigger_snapshot_count: usize,
    /// Physical selection the worker applies.
    pub compaction_type: ForgeCompactionType,
    /// Whether the leader timer expires this table's snapshots.
    pub snapshot_expiration_enabled: bool,
    /// Whether the leader timer rewrites this table's data manifests.
    pub manifest_rewrite_enabled: bool,
}

impl Default for ForgeTableSettings {
    /// Returns RisingWave's Iceberg sink defaults.
    fn default() -> Self {
        Self {
            compaction_enabled: false,
            compaction_interval: Duration::from_secs(3600),
            trigger_snapshot_count: usize::MAX,
            compaction_type: ForgeCompactionType::Full,
            snapshot_expiration_enabled: true,
            manifest_rewrite_enabled: false,
        }
    }
}

impl ForgeTableSettings {
    /// Reads one table's settings, defaulting every absent property.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when a present property does not
    /// parse, or when the interval is zero.
    pub fn from_properties(properties: &HashMap<String, String>) -> Result<Self, ForgeError> {
        let defaults = Self::default();
        let compaction_interval = Duration::from_secs(parse_or(
            properties,
            COMPACTION_INTERVAL_PROPERTY,
            defaults.compaction_interval.as_secs(),
        )?);
        if compaction_interval.is_zero() {
            return Err(ForgeError::InvalidConfig {
                detail: format!("{COMPACTION_INTERVAL_PROPERTY} must be positive"),
            });
        }
        Ok(Self {
            compaction_enabled: parse_or(
                properties,
                ENABLE_COMPACTION_PROPERTY,
                defaults.compaction_enabled,
            )?,
            compaction_interval,
            trigger_snapshot_count: parse_or(
                properties,
                TRIGGER_SNAPSHOT_COUNT_PROPERTY,
                defaults.trigger_snapshot_count,
            )?,
            compaction_type: properties
                .get(COMPACTION_TYPE_PROPERTY)
                .map_or(Ok(defaults.compaction_type), |raw| {
                    ForgeCompactionType::parse(raw)
                })?,
            snapshot_expiration_enabled: parse_or(
                properties,
                ENABLE_SNAPSHOT_EXPIRATION_PROPERTY,
                defaults.snapshot_expiration_enabled,
            )?,
            manifest_rewrite_enabled: parse_or(
                properties,
                ENABLE_MANIFEST_REWRITE_PROPERTY,
                defaults.manifest_rewrite_enabled,
            )?,
        })
    }
}

/// Parses one present property or returns its default.
///
/// # Errors
///
/// Returns [`ForgeError::InvalidConfig`] when the present value does not parse.
fn parse_or<T>(properties: &HashMap<String, String>, key: &str, default: T) -> Result<T, ForgeError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    properties.get(key).map_or(Ok(default), |raw| {
        raw.parse().map_err(|error| ForgeError::InvalidConfig {
            detail: format!("table property {key}={raw:?} is invalid: {error}"),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Absent properties are RisingWave's defaults; present ones override them.
    ///
    /// # Panics
    /// Panics when parsing or a default disagrees with the pinned reference.
    #[test]
    fn settings_default_to_risingwave_and_parse_overrides() {
        let defaults = ForgeTableSettings::from_properties(&HashMap::new()).expect("defaults");
        assert_eq!(defaults, ForgeTableSettings::default());
        assert!(!defaults.compaction_enabled && defaults.snapshot_expiration_enabled);
        assert_eq!(defaults.trigger_snapshot_count, usize::MAX);

        let declared = HashMap::from([
            (ENABLE_COMPACTION_PROPERTY.to_owned(), "true".to_owned()),
            (COMPACTION_INTERVAL_PROPERTY.to_owned(), "60".to_owned()),
            (TRIGGER_SNAPSHOT_COUNT_PROPERTY.to_owned(), "3".to_owned()),
            (
                COMPACTION_TYPE_PROPERTY.to_owned(),
                "small-files".to_owned(),
            ),
            (
                ENABLE_SNAPSHOT_EXPIRATION_PROPERTY.to_owned(),
                "false".to_owned(),
            ),
            (
                ENABLE_MANIFEST_REWRITE_PROPERTY.to_owned(),
                "true".to_owned(),
            ),
        ]);
        let settings = ForgeTableSettings::from_properties(&declared).expect("declared");
        assert!(settings.compaction_enabled && settings.manifest_rewrite_enabled);
        assert!(!settings.snapshot_expiration_enabled);
        assert_eq!(settings.compaction_interval, Duration::from_secs(60));
        assert_eq!(settings.trigger_snapshot_count, 3);
        assert_eq!(settings.compaction_type, ForgeCompactionType::SmallFiles);

        for (key, raw) in [
            (COMPACTION_INTERVAL_PROPERTY, "0"),
            (COMPACTION_TYPE_PROPERTY, "everything"),
            (ENABLE_COMPACTION_PROPERTY, "yes"),
        ] {
            let bad = HashMap::from([(key.to_owned(), raw.to_owned())]);
            assert!(
                ForgeTableSettings::from_properties(&bad).is_err(),
                "{key}={raw}"
            );
        }
    }
}
