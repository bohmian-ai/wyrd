//! Vala HTTP route handlers mounted by `wyrd-server`.

pub mod eval;

pub use eval::router as eval_router;
