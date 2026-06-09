//! In-memory drift baseline fit and scoring for Wyrd's DriftCard primitive.
//!
//! This crate consumes Arrow `RecordBatch`es directly. Parquet I/O, DataCard
//! resolution, persistence, scheduling, and HTTP wiring live in other crates.
//!
//! ## Public surface
//!
//! - `fit_psi_baseline` / `score_psi` - Population Stability Index over a feature distribution.
//! - `fit_spc_baseline` / `score_spc` - Statistical Process Control with WECO rule evaluation.
//! - `score_custom` - User-defined scalar drift against an author-supplied baseline value.
//! - `fit_baseline` / `score` - Method-dispatching helpers that read a `DriftSpec`.
//!
//! ## Error model
//!
//! Two phase-specific error enums: [`DriftFitError`] for baseline construction and
//! [`DriftScoreError`] for target scoring. Both are crate-local `thiserror` enums
//! and do not participate in the `WyrdError` derive (no HTTP surface this phase).

pub mod baseline;
pub mod custom;
pub mod error;
pub mod feature;
pub mod psi;
pub mod report;
pub mod spc;

pub use baseline::{FittedBaseline, fit_baseline, score};
pub use custom::score_custom;
pub use error::{DriftFitError, DriftScoreError};
pub use psi::{FittedPsiFeature, PsiBaseline, fit_psi_baseline, score_psi};
pub use report::{DriftReport, DriftVerdict, FeatureDriftReport};
pub use spc::{FittedSpcFeature, SpcBaseline, fit_spc_baseline, score_spc};
