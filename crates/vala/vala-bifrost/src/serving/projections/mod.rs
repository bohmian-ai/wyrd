//! Projection matching and read-time plan rewriting (slice 04).
//!
//! - [`matcher`] — IO-free freshness + fingerprint matching.
//! - [`rewrite`] — `DataFusion` logical-plan rewriter.
//!
//! The query context builder in [`super::session`] loads projections via
//! `vala_sql::queries::olap_catalog::list_by_source`, converts each row to
//! a [`matcher::ProjectionCandidate`], runs [`matcher::match_projections`],
//! then hands the chosen candidate to [`rewrite::ProjectionRewriter`].

pub mod matcher;
pub mod rewrite;

pub use matcher::{
    MatchPlan, MatchedProjection, ProjectionCandidate, ProjectionKind, match_projections,
};
pub use rewrite::ProjectionRewriter;
