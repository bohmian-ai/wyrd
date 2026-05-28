//! Python-facing model interface holders and Rust metadata conversion.

#[cfg(feature = "python")]
mod base;
#[cfg(feature = "python")]
mod handle;
#[cfg(feature = "python")]
mod helpers;
#[cfg(feature = "python")]
mod kinds;
#[cfg(feature = "python")]
mod materialize;
#[cfg(feature = "python")]
mod metadata;
pub(crate) mod options;

#[cfg(feature = "python")]
pub use base::ModelInterface;
#[cfg(feature = "python")]
pub use handle::ModelInterfaceHandle;
#[cfg(feature = "python")]
pub use kinds::{
    CatboostInterface, HuggingfaceInterface, LightgbmInterface, LightningInterface,
    SklearnInterface, TensorflowInterface, TorchInterface, XgboostInterface,
};
pub use options::{
    parse_huggingface_task, parse_sample_input_kind, parse_task_type, parse_tf_save_format,
    parse_torch_save_format,
};

#[cfg(feature = "python")]
use {pyo3::prelude::*, pyo3::types::PyModule};

/// Register model interface classes on a Python module.
#[cfg(feature = "python")]
pub fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<ModelInterface>()?;
    module.add_class::<SklearnInterface>()?;
    module.add_class::<XgboostInterface>()?;
    module.add_class::<LightgbmInterface>()?;
    module.add_class::<CatboostInterface>()?;
    module.add_class::<TorchInterface>()?;
    module.add_class::<LightningInterface>()?;
    module.add_class::<TensorflowInterface>()?;
    module.add_class::<HuggingfaceInterface>()?;
    Ok(())
}
