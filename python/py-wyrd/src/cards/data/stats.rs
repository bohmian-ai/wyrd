//! Native `DataCard` stats integration point.

/// Marker used by tests and future `PyO3` wrappers to confirm module wiring.
#[must_use]
pub fn stats_module_ready() -> bool {
    true
}
