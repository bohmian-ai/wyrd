//! Provider module — tenant filter attachment.

pub mod table;
pub mod tenant_filter;

pub use table::ReduxTableProvider;
pub use tenant_filter::attach_tenant_filter;
