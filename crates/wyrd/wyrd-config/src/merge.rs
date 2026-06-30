//! Pure merge function. No IO, no async, no allocation surprises.

use wyrd_spec::envelope::{CardKind, Metadata};

use crate::config::{KindOverride, WyrdConfig};

/// Splice workspace defaults into a card's metadata.
///
/// Precedence (most specific wins):
/// 1. Card YAML (already in `meta` — untouched if set).
/// 2. `cfg.kind_overrides[kind]`.
/// 3. `cfg.defaults`.
///
/// Maps merge per-key (insert-if-absent). Scalars fill only when
/// `None`. The card's own values are never overwritten.
///
/// `meta.name`, `meta.version`, `meta.uid`, `meta.bump`,
/// `meta.spec_hash`, and `meta.artifact_hash` are never read or
/// written by this function. The first three are author
/// identity / intent (lock L4 plus Q5 for `version`); the last three
/// are server-derived.
///
/// # Examples
///
/// ```
/// use wyrd_config::{WyrdConfig, apply_defaults};
/// use wyrd_spec::envelope::{CardKind, Metadata};
/// use wyrd_spec::ids::{CardName, SpaceName};
///
/// let mut cfg = WyrdConfig::empty();
/// cfg.defaults.space = Some(SpaceName::new("prod").unwrap());
///
/// let mut meta = Metadata {
///     name: CardName::new("churn-classifier").unwrap(),
///     version: None,
///     bump: None,
///     space: None,
///     uid: None,
///     labels: Default::default(),
///     annotations: Default::default(),
///     spec_hash: None,
///     artifact_hash: None,
///     origin: None,
/// };
/// apply_defaults(&mut meta, &CardKind::Model, &cfg);
/// assert_eq!(meta.space.as_ref().map(|s| s.as_str()), Some("prod"));
/// // version is never synthesized by the loader (Q5 / L7).
/// assert!(meta.version.is_none());
/// ```
pub fn apply_defaults(meta: &mut Metadata, kind: &CardKind, cfg: &WyrdConfig) {
    let kind_override: Option<&KindOverride> = cfg.kind_overrides.get(kind);

    // Scalar: space
    if meta.space.is_none() {
        if let Some(s) = kind_override.and_then(|k| k.space.clone()) {
            meta.space = Some(s);
        } else if let Some(s) = cfg.defaults.space.clone() {
            meta.space = Some(s);
        }
    }

    // Maps: kind first, then defaults; insert-if-absent so the
    // card's own keys always win.
    if let Some(k) = kind_override {
        for (key, val) in &k.labels {
            meta.labels
                .entry(key.clone())
                .or_insert_with(|| val.clone());
        }
        for (key, val) in &k.annotations {
            meta.annotations
                .entry(key.clone())
                .or_insert_with(|| val.clone());
        }
    }
    for (key, val) in &cfg.defaults.labels {
        meta.labels
            .entry(key.clone())
            .or_insert_with(|| val.clone());
    }
    for (key, val) in &cfg.defaults.annotations {
        meta.annotations
            .entry(key.clone())
            .or_insert_with(|| val.clone());
    }

    // Author-identity / intent fields are never touched. Server-
    // derived fields are never touched either.
    let _ = (
        &meta.name,
        &meta.version,
        &meta.uid,
        &meta.bump,
        &meta.spec_hash,
        &meta.artifact_hash,
    );
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::metadata::{LabelKey, LabelValue};

    fn label_pair(k: &str, v: &str) -> (LabelKey, LabelValue) {
        (LabelKey::new(k).unwrap(), LabelValue::new(v).unwrap())
    }

    fn fresh_meta(name: &str) -> Metadata {
        Metadata {
            name: CardName::new(name).unwrap(),
            version: None,
            bump: None,
            space: None,
            uid: None,
            labels: BTreeMap::default(),
            annotations: BTreeMap::default(),
            spec_hash: None,
            artifact_hash: None,
            origin: None,
        }
    }

    #[test]
    fn apply_defaults_is_idempotent() {
        let mut cfg = WyrdConfig::empty();
        cfg.defaults.space = Some(SpaceName::new("prod").unwrap());
        let (k, v) = label_pair("team", "ml");
        cfg.defaults.labels.insert(k, v);

        let mut meta = fresh_meta("card");
        apply_defaults(&mut meta, &CardKind::Model, &cfg);
        let snapshot = meta.clone();
        apply_defaults(&mut meta, &CardKind::Model, &cfg);
        assert_eq!(meta.space, snapshot.space);
        assert_eq!(meta.labels, snapshot.labels);
    }

    #[test]
    fn card_label_wins_over_default() {
        let mut cfg = WyrdConfig::empty();
        let (k, v) = label_pair("team", "platform");
        cfg.defaults.labels.insert(k.clone(), v);

        let mut meta = fresh_meta("card");
        meta.labels
            .insert(k.clone(), LabelValue::new("churn-ml").unwrap());
        apply_defaults(&mut meta, &CardKind::Model, &cfg);
        assert_eq!(
            meta.labels.get(&k).map(LabelValue::as_str),
            Some("churn-ml")
        );
    }
}
