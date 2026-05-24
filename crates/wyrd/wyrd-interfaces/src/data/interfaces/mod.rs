//! Python-facing data interface holders and Rust metadata conversion.

mod base;
#[cfg(feature = "python")]
mod handle;
#[cfg(feature = "python")]
mod helpers;
mod kinds;
mod materialize;
mod metadata;
mod options;

pub use base::DataInterface;
#[cfg(feature = "python")]
#[allow(unused_imports)]
pub(crate) use handle::DataInterfaceHandle;
pub use kinds::{
    ArrowInterface, CustomDataInterface, HuggingfaceInterface, ImageInterface, JsonlInterface,
    NumpyInterface, PandasInterface, ParquetInterface, PolarsInterface, SqlInterface,
    TextInterface, TorchInterface,
};
pub use options::{
    parse_arrow_format, parse_color_mode, parse_image_format, parse_jsonl_compression,
    parse_numpy_format, parse_parquet_compression, parse_torch_save_format,
};

#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::types::PyModule;

/// Register data interface classes on a Python module.
#[cfg(feature = "python")]
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<DataInterface>()?;
    module.add_class::<PandasInterface>()?;
    module.add_class::<PolarsInterface>()?;
    module.add_class::<ArrowInterface>()?;
    module.add_class::<ParquetInterface>()?;
    module.add_class::<NumpyInterface>()?;
    module.add_class::<TorchInterface>()?;
    module.add_class::<SqlInterface>()?;
    module.add_class::<JsonlInterface>()?;
    module.add_class::<ImageInterface>()?;
    module.add_class::<TextInterface>()?;
    module.add_class::<HuggingfaceInterface>()?;
    module.add_class::<CustomDataInterface>()?;
    Ok(())
}
