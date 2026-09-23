//! JSON-safe serde for PSI bin edges.
//!
//! The outer numeric bins are open, so their edges are `-inf`/`+inf`, which a
//! JSON number cannot hold. A finite edge serializes as a number, an infinite
//! edge as the string `"-inf"` or `"inf"`, and an absent (categorical) edge as
//! `null`, so a fitted baseline round-trips exactly through JSON storage.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One stored edge value.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Edge {
    /// A finite edge.
    Finite(f64),
    /// An infinite edge, `"-inf"` or `"inf"`.
    Infinite(String),
}

/// Serialize an optional edge, spelling infinities as strings.
///
/// # Errors
/// Returns the serializer's error.
pub(super) fn serialize<S: Serializer>(
    value: &Option<f64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    value
        .map(|edge| match edge {
            edge if edge.is_finite() => Edge::Finite(edge),
            edge if edge.is_sign_negative() => Edge::Infinite("-inf".to_owned()),
            _ => Edge::Infinite("inf".to_owned()),
        })
        .serialize(serializer)
}

/// Deserialize an optional edge written by [`serialize`].
///
/// # Errors
/// Returns a custom error for a string other than `"-inf"` or `"inf"`.
pub(super) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<f64>, D::Error> {
    Option::<Edge>::deserialize(deserializer)?
        .map(|edge| match edge {
            Edge::Finite(edge) => Ok(edge),
            Edge::Infinite(text) if text == "-inf" => Ok(f64::NEG_INFINITY),
            Edge::Infinite(text) if text == "inf" => Ok(f64::INFINITY),
            Edge::Infinite(text) => Err(serde::de::Error::custom(format!(
                "invalid bin edge {text:?}"
            ))),
        })
        .transpose()
}
