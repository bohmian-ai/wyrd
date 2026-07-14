//! Semver range expressions.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::block::VersionBlock;
use crate::bounds::{SemverTriple, VersionBounds};
use crate::error::VersionError;

/// Version range expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct VersionRange(String);

impl VersionRange {
    /// Parse a semantic version requirement, storing the expression as-is.
    ///
    /// Accepts any form accepted by the `semver` crate, including short forms
    /// like `^1.2` or `~1`. For caller-facing inputs where you want a
    /// canonical three-segment stored form, use [`VersionRange::parse_loose`].
    ///
    /// # Errors
    /// Returns an error when `value` is empty or not a valid semver requirement.
    pub fn parse(value: impl Into<String>) -> Result<Self, VersionError> {
        let value = value.into();
        if value.is_empty() {
            return Err(VersionError::EmptyVersion);
        }
        semver::VersionReq::parse(&value)
            .map_err(|source| VersionError::InvalidRange { source })?;
        Ok(Self(value))
    }

    /// Parse a range expression that may include Wyrd short-form dialects
    /// (`^1.2`, `~1.2`, `1.*`, partial bare versions like `1` or `1.2`).
    ///
    /// Normalizes to a form acceptable to `semver::VersionReq`, so the
    /// resulting `VersionRange` supports `.matches()` against `VersionBlock`s.
    /// The stored canonical form is the normalized string; `Display` shows the
    /// normalized value, not the original input.
    ///
    /// Strict forms (`^1.2.3`, `~1.2.3`, `>=1.0,<2.0`, `1.2.3`, `*`) pass
    /// through unchanged.
    ///
    /// # Errors
    /// Returns an error when `value` is empty or cannot be normalized to a
    /// valid semver requirement.
    pub fn parse_loose(value: impl Into<String>) -> Result<Self, VersionError> {
        let value = value.into();
        if value.is_empty() {
            return Err(VersionError::EmptyVersion);
        }
        let normalized = normalize_loose(&value)?;
        semver::VersionReq::parse(&normalized)
            .map_err(|source| VersionError::InvalidRange { source })?;
        Ok(Self(normalized))
    }

    /// Borrow as a string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check whether a concrete version matches this range.
    pub fn matches(&self, version: &VersionBlock) -> bool {
        let range = semver::VersionReq::parse(&self.0)
            .expect("VersionRange invariant: stored value is valid");
        range.matches(
            &version
                .semver()
                .expect("VersionBlock invariant: stored value is valid"),
        )
    }

    /// Returns true only when this range was parsed from a bare partial input
    /// (`"1"` or `"1.2"`), normalized to `"^X"` or `"~X.Y"` respectively.
    ///
    /// Used by register-path validators that accept partial-pin scopes but
    /// reject explicit comparator syntax (`^1.0.0`, `~1.0.0`, `1.*`, `>=1.0`).
    /// All other inputs — including explicit `^1` (normalized to `^1.0.0`) or
    /// `~1.0` (normalized to `~1.0.0`) — return false.
    #[must_use]
    pub fn is_loose_partial(&self) -> bool {
        let s = self.0.as_str();
        let mut chars = s.chars();
        let prefix = match chars.next() {
            Some('^') | Some('~') => s.as_bytes()[0],
            _ => return false,
        };
        let body = &s[1..];
        if body.is_empty()
            || body.contains('-')
            || body.contains('+')
            || body.contains('*')
            || body.contains(',')
        {
            return false;
        }
        let parts: Vec<&str> = body.split('.').collect();
        let all_digits = parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
        if !all_digits {
            return false;
        }
        match prefix {
            b'^' => parts.len() == 1,
            b'~' => parts.len() == 2,
            _ => false,
        }
    }

    /// Compute the (inclusive lower, optional upper) semver triple bounds for
    /// this range expression, for use in SQL predicates of the form
    /// `(major, minor, patch) >= lower AND (major, minor, patch) < upper`.
    ///
    /// Supported dialect (Wyrd prefix grammar):
    /// - `^1.2.3` → `[1.2.3, 2.0.0)`
    /// - `~1.2.3` → `[1.2.3, 1.3.0)`
    /// - `1.2.*`  → `[1.2.0, 1.3.0)`
    /// - `1.*`    → `[1.0.0, 2.0.0)`
    /// - `*`      → `[0.0.0, unbounded)`
    /// - `1.2`    → `[1.2.0, 1.3.0)` (partial-pin semantics)
    /// - `1`      → `[1.0.0, 2.0.0)`
    /// - `1.2.3`  → `[1.2.3, 1.2.3]` (`upper_inclusive = true`)
    ///
    /// Range expressions with multiple comparators or holes (e.g.
    /// `>=1.0,<2.0,!=1.5.0`) are not bound-representable and return
    /// [`VersionError::InvalidRange`].
    ///
    /// # Errors
    /// Returns an error when the range is empty, not representable as a single
    /// half-open interval, or would overflow a `u64` upper bound.
    pub fn to_bounds(&self) -> Result<VersionBounds, VersionError> {
        bounds_from_str(&self.0)
    }
}

impl Default for VersionRange {
    fn default() -> Self {
        Self("*".to_string())
    }
}

impl fmt::Display for VersionRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for VersionRange {
    type Err = VersionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

fn normalize_loose(value: &str) -> Result<String, VersionError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(VersionError::EmptyVersion);
    }

    // Pass-through: comma-comparator chains, full wildcards, and explicit
    // comparator-led expressions are handed straight to semver::VersionReq.
    if trimmed == "*"
        || trimmed.contains(',')
        || trimmed.starts_with(">=")
        || trimmed.starts_with("<=")
        || trimmed.starts_with('>')
        || trimmed.starts_with('<')
        || trimmed.starts_with('=')
    {
        return Ok(trimmed.to_string());
    }

    // Strip leading ^ / ~ prefix to inspect the body. Both prefixes wrap the
    // body in semver-crate notation that we'll reconstruct after expansion.
    let (prefix, body) = match trimmed.chars().next() {
        Some('^') => ("^", &trimmed[1..]),
        Some('~') => ("~", &trimmed[1..]),
        _ => ("", trimmed),
    };

    if body.is_empty() {
        return Err(VersionError::EmptyVersion);
    }

    // X.Y.* / X.* — pass through to semver::VersionReq, which accepts these
    // natively. A wildcard mixed with ^ or ~ is rejected.
    if body.contains('*') {
        if !prefix.is_empty() {
            return Err(VersionError::NotRepresentable {
                reason: "wildcard cannot be combined with ^ or ~",
            });
        }
        return Ok(body.to_string());
    }

    // Bodies carrying pre-release or build metadata are passed through unchanged;
    // they're already fully-qualified for semver::VersionReq.
    if body.contains('-') || body.contains('+') {
        return Ok(format!("{prefix}{body}"));
    }

    let parts: Vec<&str> = body.split('.').collect();

    // ^X / ~X partial forms pad to .0[.0] without changing the prefix.
    if !prefix.is_empty() {
        let expanded = match parts.len() {
            1 => format!("{}.0.0", parts[0]),
            2 => format!("{}.{}.0", parts[0], parts[1]),
            3 => body.to_string(),
            _ => {
                return Err(VersionError::NotRepresentable {
                    reason: "too many version segments",
                });
            }
        };
        return Ok(format!("{prefix}{expanded}"));
    }

    // Bare partials map to partial-pin semantics:
    //   "X"     → "^X"    (lock major)
    //   "X.Y"   → "~X.Y"  (lock major+minor)
    //   "X.Y.Z" → "=X.Y.Z" (exact)
    match parts.len() {
        1 => Ok(format!("^{}", parts[0])),
        2 => Ok(format!("~{}.{}", parts[0], parts[1])),
        3 => Ok(format!("={}.{}.{}", parts[0], parts[1], parts[2])),
        _ => Err(VersionError::NotRepresentable {
            reason: "too many version segments",
        }),
    }
}

fn bounds_from_str(raw: &str) -> Result<VersionBounds, VersionError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(VersionError::EmptyVersion);
    }

    if trimmed == "*" {
        return Ok(VersionBounds {
            lower: SemverTriple::ZERO,
            upper: None,
            upper_inclusive: false,
        });
    }

    if trimmed.contains(',') {
        return Err(VersionError::NotRepresentable {
            reason: "multi-comparator ranges are not bound-representable",
        });
    }

    let (prefix, body) = match trimmed.chars().next() {
        Some('^') => (Prefix::Caret, &trimmed[1..]),
        Some('~') => (Prefix::Tilde, &trimmed[1..]),
        Some('=') => (Prefix::Eq, &trimmed[1..]),
        _ => (Prefix::None, trimmed),
    };

    if body.is_empty() {
        return Err(VersionError::EmptyVersion);
    }

    if body.contains('-') || body.contains('+') {
        return Err(VersionError::NotRepresentable {
            reason: "pre-release and build-metadata ranges cannot be expressed as SQL bounds",
        });
    }

    let segments: Vec<Segment> = body
        .split('.')
        .map(|part| {
            if part == "*" {
                Ok(Segment::Wildcard)
            } else {
                part.parse::<u64>()
                    .map(Segment::Num)
                    .map_err(|_| VersionError::NotRepresentable {
                        reason: "invalid character in version segment",
                    })
            }
        })
        .collect::<Result<_, _>>()?;

    if segments.is_empty() || segments.len() > 3 {
        return Err(VersionError::NotRepresentable {
            reason: "too many version segments",
        });
    }

    match prefix {
        Prefix::Caret => caret_bounds(&segments),
        Prefix::Tilde => tilde_bounds(&segments),
        Prefix::Eq => eq_bounds(&segments),
        Prefix::None => plain_bounds(&segments),
    }
}

enum Prefix {
    Caret,
    Tilde,
    Eq,
    None,
}

enum Segment {
    Num(u64),
    Wildcard,
}

fn segment_num(segment: &Segment) -> Result<u64, VersionError> {
    match segment {
        Segment::Num(n) => Ok(*n),
        Segment::Wildcard => Err(VersionError::NotRepresentable {
            reason: "wildcard is not valid here",
        }),
    }
}

fn checked_add_one(value: u64) -> Result<u64, VersionError> {
    value.checked_add(1).ok_or(VersionError::BoundsOverflow)
}

fn plain_bounds(segments: &[Segment]) -> Result<VersionBounds, VersionError> {
    // Find the first wildcard (if any). Everything to the right must also be
    // wildcard or absent.
    let first_wild = segments.iter().position(|s| matches!(s, Segment::Wildcard));
    if let Some(wild_at) = first_wild {
        if segments[wild_at..]
            .iter()
            .any(|s| matches!(s, Segment::Num(_)))
        {
            return Err(VersionError::NotRepresentable {
                reason: "wildcard must be in a trailing position",
            });
        }
        // wild_at == 0 → bare "*" (handled earlier); shouldn't reach.
        let major = segment_num(&segments[0])?;
        if wild_at == 1 {
            // X.*
            return Ok(VersionBounds {
                lower: SemverTriple::new(major, 0, 0),
                upper: Some(SemverTriple::new(checked_add_one(major)?, 0, 0)),
                upper_inclusive: false,
            });
        }
        // wild_at == 2 → X.Y.*
        let minor = segment_num(&segments[1])?;
        return Ok(VersionBounds {
            lower: SemverTriple::new(major, minor, 0),
            upper: Some(SemverTriple::new(major, checked_add_one(minor)?, 0)),
            upper_inclusive: false,
        });
    }

    let major = segment_num(&segments[0])?;
    match segments.len() {
        1 => Ok(VersionBounds {
            lower: SemverTriple::new(major, 0, 0),
            upper: Some(SemverTriple::new(checked_add_one(major)?, 0, 0)),
            upper_inclusive: false,
        }),
        2 => {
            let minor = segment_num(&segments[1])?;
            Ok(VersionBounds {
                lower: SemverTriple::new(major, minor, 0),
                upper: Some(SemverTriple::new(major, checked_add_one(minor)?, 0)),
                upper_inclusive: false,
            })
        }
        3 => {
            let minor = segment_num(&segments[1])?;
            let patch = segment_num(&segments[2])?;
            Ok(VersionBounds {
                lower: SemverTriple::new(major, minor, patch),
                upper: Some(SemverTriple::new(major, minor, patch)),
                upper_inclusive: true,
            })
        }
        _ => unreachable!("segments length validated"),
    }
}

fn caret_bounds(segments: &[Segment]) -> Result<VersionBounds, VersionError> {
    // ^X.Y[.Z] — standard three-way caret rule:
    //   major != 0 → [X.Y.Z, X+1.0.0)
    //   major == 0, minor != 0 → [0.Y.Z, 0.Y+1.0)
    //   major == 0, minor == 0 → [0.0.Z, 0.0.Z+1)
    if segments.iter().any(|s| matches!(s, Segment::Wildcard)) {
        return Err(VersionError::NotRepresentable {
            reason: "wildcard cannot be combined with ^ or ~",
        });
    }
    let major = segment_num(&segments[0])?;
    let minor = segments.get(1).map(segment_num).transpose()?.unwrap_or(0);
    let patch = segments.get(2).map(segment_num).transpose()?.unwrap_or(0);
    let upper = if major != 0 {
        SemverTriple::new(checked_add_one(major)?, 0, 0)
    } else if minor != 0 {
        SemverTriple::new(0, checked_add_one(minor)?, 0)
    } else {
        SemverTriple::new(0, 0, checked_add_one(patch)?)
    };
    Ok(VersionBounds {
        lower: SemverTriple::new(major, minor, patch),
        upper: Some(upper),
        upper_inclusive: false,
    })
}

fn eq_bounds(segments: &[Segment]) -> Result<VersionBounds, VersionError> {
    // =X.Y.Z — exact pin. Only the fully-qualified triple form is representable
    // as a single SemverTriple; partial forms (=X, =X.Y) are rejected because
    // the lower bound is ambiguous (does =1.2 mean any 1.2.* or only 1.2.0?).
    if segments.iter().any(|s| matches!(s, Segment::Wildcard)) || segments.len() != 3 {
        return Err(VersionError::NotRepresentable {
            reason: "wildcard cannot be combined with ^ or ~",
        });
    }
    let major = segment_num(&segments[0])?;
    let minor = segment_num(&segments[1])?;
    let patch = segment_num(&segments[2])?;
    let triple = SemverTriple::new(major, minor, patch);
    Ok(VersionBounds {
        lower: triple,
        upper: Some(triple),
        upper_inclusive: true,
    })
}

fn tilde_bounds(segments: &[Segment]) -> Result<VersionBounds, VersionError> {
    // ~X.Y[.Z] — lock major+minor, allow patch to vary.
    if segments.iter().any(|s| matches!(s, Segment::Wildcard)) {
        return Err(VersionError::NotRepresentable {
            reason: "wildcard cannot be combined with ^ or ~",
        });
    }
    let major = segment_num(&segments[0])?;
    let minor = segments.get(1).map(segment_num).transpose()?.unwrap_or(0);
    let patch = segments.get(2).map(segment_num).transpose()?.unwrap_or(0);
    Ok(VersionBounds {
        lower: SemverTriple::new(major, minor, patch),
        upper: Some(SemverTriple::new(major, checked_add_one(minor)?, 0)),
        upper_inclusive: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_strict_round_trip() {
        let range = VersionRange::parse(">=1.0.0, <2.0.0").unwrap();
        assert_eq!(range.as_str(), ">=1.0.0, <2.0.0");
    }

    #[test]
    fn parse_rejects_empty() {
        let err = VersionRange::parse("").unwrap_err();
        assert!(matches!(err, VersionError::EmptyVersion));
    }

    #[test]
    fn default_is_wildcard() {
        assert_eq!(VersionRange::default().as_str(), "*");
    }

    #[test]
    fn matches_caret_full() {
        let range = VersionRange::parse("^1.2.3").unwrap();
        assert!(range.matches(&VersionBlock::parse("1.2.3").unwrap()));
        assert!(range.matches(&VersionBlock::parse("1.9.0").unwrap()));
        assert!(!range.matches(&VersionBlock::parse("2.0.0").unwrap()));
    }

    #[test]
    fn matches_tilde_full() {
        let range = VersionRange::parse("~1.2.3").unwrap();
        assert!(range.matches(&VersionBlock::parse("1.2.3").unwrap()));
        assert!(range.matches(&VersionBlock::parse("1.2.9").unwrap()));
        assert!(!range.matches(&VersionBlock::parse("1.3.0").unwrap()));
    }

    #[test]
    fn parse_loose_normalizes_partial_minor() {
        let range = VersionRange::parse_loose("1.2").unwrap();
        assert!(range.matches(&VersionBlock::parse("1.2.0").unwrap()));
        assert!(range.matches(&VersionBlock::parse("1.2.5").unwrap()));
        assert!(!range.matches(&VersionBlock::parse("1.3.0").unwrap()));
    }

    #[test]
    fn parse_loose_normalizes_partial_major() {
        let range = VersionRange::parse_loose("1").unwrap();
        assert!(range.matches(&VersionBlock::parse("1.0.0").unwrap()));
        assert!(range.matches(&VersionBlock::parse("1.9.9").unwrap()));
        assert!(!range.matches(&VersionBlock::parse("2.0.0").unwrap()));
    }

    #[test]
    fn parse_loose_normalizes_star_minor() {
        let range = VersionRange::parse_loose("1.*").unwrap();
        assert!(range.matches(&VersionBlock::parse("1.0.0").unwrap()));
        assert!(range.matches(&VersionBlock::parse("1.9.9").unwrap()));
        assert!(!range.matches(&VersionBlock::parse("2.0.0").unwrap()));
    }

    #[test]
    fn parse_loose_caret_short() {
        let range = VersionRange::parse_loose("^1.2").unwrap();
        assert!(range.matches(&VersionBlock::parse("1.2.0").unwrap()));
        assert!(range.matches(&VersionBlock::parse("1.9.0").unwrap()));
        assert!(!range.matches(&VersionBlock::parse("2.0.0").unwrap()));
    }

    #[test]
    fn parse_loose_tilde_short() {
        let range = VersionRange::parse_loose("~1.2").unwrap();
        assert!(range.matches(&VersionBlock::parse("1.2.0").unwrap()));
        assert!(range.matches(&VersionBlock::parse("1.2.9").unwrap()));
        assert!(!range.matches(&VersionBlock::parse("1.3.0").unwrap()));
    }

    #[test]
    fn parse_accepts_short_caret_form() {
        assert!(VersionRange::parse("^1.2").is_ok());
    }

    #[test]
    fn parse_loose_passes_through_strict_caret() {
        let range = VersionRange::parse_loose("^1.2.3").unwrap();
        assert_eq!(range.as_str(), "^1.2.3");
    }

    #[test]
    fn parse_loose_passes_through_comparator_chain() {
        let range = VersionRange::parse_loose(">=1.0.0, <2.0.0").unwrap();
        assert_eq!(range.as_str(), ">=1.0.0, <2.0.0");
    }

    #[test]
    fn parse_loose_rejects_empty() {
        let err = VersionRange::parse_loose("").unwrap_err();
        assert!(matches!(err, VersionError::EmptyVersion));
    }

    #[test]
    fn to_bounds_caret_full() {
        let bounds = VersionRange::parse("^1.2.3").unwrap().to_bounds().unwrap();
        assert_eq!(bounds.lower, SemverTriple::new(1, 2, 3));
        assert_eq!(bounds.upper, Some(SemverTriple::new(2, 0, 0)));
        assert!(!bounds.upper_inclusive);
    }

    #[test]
    fn to_bounds_caret_partial() {
        let bounds = VersionRange::parse_loose("^1.2")
            .unwrap()
            .to_bounds()
            .unwrap();
        assert_eq!(bounds.lower, SemverTriple::new(1, 2, 0));
        assert_eq!(bounds.upper, Some(SemverTriple::new(2, 0, 0)));
    }

    #[test]
    fn to_bounds_tilde_full() {
        let bounds = VersionRange::parse("~1.2.3").unwrap().to_bounds().unwrap();
        assert_eq!(bounds.lower, SemverTriple::new(1, 2, 3));
        assert_eq!(bounds.upper, Some(SemverTriple::new(1, 3, 0)));
        assert!(!bounds.upper_inclusive);
    }

    #[test]
    fn to_bounds_star_minor() {
        let bounds = VersionRange::parse_loose("1.*")
            .unwrap()
            .to_bounds()
            .unwrap();
        assert_eq!(bounds.lower, SemverTriple::new(1, 0, 0));
        assert_eq!(bounds.upper, Some(SemverTriple::new(2, 0, 0)));
        assert!(!bounds.upper_inclusive);
    }

    #[test]
    fn to_bounds_star_patch() {
        let bounds = VersionRange::parse_loose("1.2.*")
            .unwrap()
            .to_bounds()
            .unwrap();
        assert_eq!(bounds.lower, SemverTriple::new(1, 2, 0));
        assert_eq!(bounds.upper, Some(SemverTriple::new(1, 3, 0)));
    }

    #[test]
    fn to_bounds_unbounded() {
        let bounds = VersionRange::parse("*").unwrap().to_bounds().unwrap();
        assert_eq!(bounds.lower, SemverTriple::ZERO);
        assert_eq!(bounds.upper, None);
        assert!(!bounds.upper_inclusive);
    }

    #[test]
    fn to_bounds_partial_minor() {
        let bounds = VersionRange::parse_loose("1.2")
            .unwrap()
            .to_bounds()
            .unwrap();
        assert_eq!(bounds.lower, SemverTriple::new(1, 2, 0));
        assert_eq!(bounds.upper, Some(SemverTriple::new(1, 3, 0)));
    }

    #[test]
    fn to_bounds_partial_major() {
        let bounds = VersionRange::parse_loose("1").unwrap().to_bounds().unwrap();
        assert_eq!(bounds.lower, SemverTriple::new(1, 0, 0));
        assert_eq!(bounds.upper, Some(SemverTriple::new(2, 0, 0)));
    }

    #[test]
    fn to_bounds_exact_pin() {
        let bounds = VersionRange::parse_loose("1.2.3")
            .unwrap()
            .to_bounds()
            .unwrap();
        assert_eq!(bounds.lower, SemverTriple::new(1, 2, 3));
        assert_eq!(bounds.upper, Some(SemverTriple::new(1, 2, 3)));
        assert!(bounds.upper_inclusive);
    }

    #[test]
    fn to_bounds_rejects_multi_comparator() {
        let err = VersionRange::parse(">=1.0.0, <2.0.0")
            .unwrap()
            .to_bounds()
            .unwrap_err();
        assert!(matches!(err, VersionError::NotRepresentable { .. }));
    }

    #[test]
    fn to_bounds_caret_pre_1_minor() {
        let bounds = VersionRange::parse("^0.1.2").unwrap().to_bounds().unwrap();
        assert_eq!(bounds.lower, SemverTriple::new(0, 1, 2));
        assert_eq!(bounds.upper, Some(SemverTriple::new(0, 2, 0)));
        assert!(!bounds.upper_inclusive);
    }

    #[test]
    fn to_bounds_caret_pre_1_patch() {
        let bounds = VersionRange::parse("^0.0.3").unwrap().to_bounds().unwrap();
        assert_eq!(bounds.lower, SemverTriple::new(0, 0, 3));
        assert_eq!(bounds.upper, Some(SemverTriple::new(0, 0, 4)));
        assert!(!bounds.upper_inclusive);
    }

    #[test]
    fn to_bounds_rejects_prerelease() {
        let err = VersionRange::parse("^1.0.0-alpha.1")
            .unwrap()
            .to_bounds()
            .unwrap_err();
        assert!(matches!(err, VersionError::NotRepresentable { .. }));
    }

    #[test]
    fn is_loose_partial_bare_major() {
        assert!(VersionRange::parse_loose("1").unwrap().is_loose_partial());
    }

    #[test]
    fn is_loose_partial_bare_minor() {
        assert!(VersionRange::parse_loose("1.2").unwrap().is_loose_partial());
    }

    #[test]
    fn is_loose_partial_rejects_explicit_caret() {
        assert!(!VersionRange::parse_loose("^1").unwrap().is_loose_partial());
        assert!(
            !VersionRange::parse_loose("^1.2")
                .unwrap()
                .is_loose_partial()
        );
        assert!(
            !VersionRange::parse_loose("^1.2.3")
                .unwrap()
                .is_loose_partial()
        );
    }

    #[test]
    fn is_loose_partial_rejects_explicit_tilde() {
        assert!(!VersionRange::parse_loose("~1").unwrap().is_loose_partial());
        assert!(
            !VersionRange::parse_loose("~1.2")
                .unwrap()
                .is_loose_partial()
        );
        assert!(
            !VersionRange::parse_loose("~1.2.3")
                .unwrap()
                .is_loose_partial()
        );
    }

    #[test]
    fn is_loose_partial_rejects_wildcard() {
        assert!(!VersionRange::parse_loose("*").unwrap().is_loose_partial());
        assert!(!VersionRange::parse_loose("1.*").unwrap().is_loose_partial());
        assert!(
            !VersionRange::parse_loose("1.2.*")
                .unwrap()
                .is_loose_partial()
        );
    }

    #[test]
    fn is_loose_partial_rejects_comparator_chain() {
        assert!(
            !VersionRange::parse(">=1.0.0, <2.0.0")
                .unwrap()
                .is_loose_partial()
        );
    }

    #[test]
    fn is_loose_partial_rejects_full_triple() {
        // bare "1.2.3" normalizes to "=1.2.3" → no '^' or '~' prefix → false.
        assert!(
            !VersionRange::parse_loose("1.2.3")
                .unwrap()
                .is_loose_partial()
        );
    }
}
