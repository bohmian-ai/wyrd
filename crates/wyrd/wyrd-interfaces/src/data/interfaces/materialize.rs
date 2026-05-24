use std::fs;
use std::path::Path;

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::data::dtype;
use crate::data::interfaces::helpers::{
    bool_kwarg, copy_manifest_entries, data_stats_for_file, data_stats_for_path, ensure_parent_dir,
    numpy_no_pickle_kwargs, numpy_npz_value_kwargs, optional_schema_for_interface,
    pandas_to_parquet_kwargs, parquet_compression_kwargs, pyarrow_engine_kwargs,
    require_local_file, require_local_path, torch_to_safetensor_map, torch_weights_only_kwargs,
    write_json_sorted,
};
use crate::data::interfaces::kinds::{
    ArrowInterface, HuggingfaceInterface, ImageInterface, JsonlInterface, NumpyInterface,
    PandasInterface, ParquetInterface, PolarsInterface, SqlInterface, TextInterface,
    TorchInterface,
};
use crate::data::interfaces::options::{jsonl_compression_token, parquet_compression_token};
use crate::data::io::{
    huggingface_pointer, image_manifest_from_data, image_manifest_schema, manifest_json_to_py,
    pointer_to_kwargs, read_huggingface_pointer, read_jsonl_to_py, serde_json_file_to_py,
    sql_logic_from_data, text_manifest_from_data, text_manifest_schema, write_jsonl_normalized,
};
use crate::data::layout::LocalArtifactLayout;
use crate::error::{CardPyResult, WyrdPyError};
use wyrd_spec::card::data::{
    ArrowFormat, DataInterface as RustDataInterface, DataSchema, DataStats, NumpyFormat,
    TorchSaveFormat,
};

#[cfg(feature = "python")]
impl PandasInterface {
    /// Save the held pandas dataframe to the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for pandas calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future pandas-specific save options.
    ///
    /// # Returns
    ///
    /// File statistics for `data/data.parquet`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, the object is not a pandas
    /// DataFrame, compression is invalid, pandas parquet writing fails, schema
    /// inference fails, or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let source = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source("PandasInterface.save requires a pandas.DataFrame")
        })?;
        let data = source.bind(py);
        dtype::ensure_pandas_dataframe(py, data)?;
        let meta = self.to_rust(py)?;
        let compression = parquet_compression_token(meta.compression);
        let absolute_path = RustDataInterface::Pandas(meta).artifact_path(path)?;
        ensure_parent_dir(&absolute_path)?;
        let kwargs = pandas_to_parquet_kwargs(py, compression)?;
        data.call_method("to_parquet", (&absolute_path,), Some(&kwargs))?;
        let schema = dtype::infer_schema_for_interface(py, data, "Pandas")?;
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load a pandas dataframe from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for pandas calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future pandas-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when `data/data.parquet` is missing or pandas cannot
    /// read the parquet artifact.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let absolute_path = RustDataInterface::Pandas(self.to_rust(py)?).artifact_path(path)?;
        require_local_file(&absolute_path)?;
        let kwargs = pyarrow_engine_kwargs(py)?;
        let pandas = py.import("pandas")?;
        let loaded = pandas.call_method("read_parquet", (&absolute_path,), Some(&kwargs))?;
        self.data = Some(loaded.unbind());
        Ok(())
    }
}
#[cfg(feature = "python")]
impl PolarsInterface {
    /// Save the held polars dataframe to the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for polars calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future polars-specific save options.
    ///
    /// # Returns
    ///
    /// File statistics for `data/data.parquet`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, the object is not a polars
    /// DataFrame, compression is invalid, polars parquet writing fails, schema
    /// inference fails, or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let source = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source("PolarsInterface.save requires a polars.DataFrame")
        })?;
        let data = source.bind(py);
        dtype::ensure_polars_dataframe(py, data)?;
        let meta = self.to_rust(py)?;
        let compression = parquet_compression_token(meta.compression);
        let absolute_path = RustDataInterface::Polars(meta).artifact_path(path)?;
        ensure_parent_dir(&absolute_path)?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("compression", compression)?;
        data.call_method("write_parquet", (&absolute_path,), Some(&kwargs))?;
        let schema = dtype::infer_schema_for_interface(py, data, "Polars")?;
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load a polars dataframe from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for polars calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future polars-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when `data/data.parquet` is missing or polars cannot
    /// read the parquet artifact.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let absolute_path = RustDataInterface::Polars(self.to_rust(py)?).artifact_path(path)?;
        require_local_file(&absolute_path)?;
        let polars = py.import("polars")?;
        self.data = Some(
            polars
                .call_method1("read_parquet", (&absolute_path,))?
                .unbind(),
        );
        Ok(())
    }
}

#[cfg(feature = "python")]
impl ArrowInterface {
    /// Save the held PyArrow table to the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for PyArrow calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future Arrow-specific save options.
    ///
    /// # Returns
    ///
    /// File statistics for `data/data.parquet` or `data/data.arrow`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, the object is not a
    /// `pyarrow.Table`, format is invalid, PyArrow serialization fails, schema
    /// inference fails, or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let source = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source("ArrowInterface.save requires a pyarrow.Table")
        })?;
        let data = source.bind(py);
        dtype::ensure_pyarrow_table(py, data)?;
        let meta = self.to_rust(py)?;
        let format = meta.format;
        let absolute_path = RustDataInterface::Arrow(meta).artifact_path(path)?;
        ensure_parent_dir(&absolute_path)?;
        match format {
            ArrowFormat::Parquet => {
                let parquet = py.import("pyarrow.parquet")?;
                parquet.call_method1("write_table", (data, &absolute_path))?;
            }
            ArrowFormat::Ipc => {
                let pyarrow = py.import("pyarrow")?;
                let ipc = py.import("pyarrow.ipc")?;
                let sink = pyarrow
                    .call_method1("OSFile", (absolute_path.to_string_lossy().as_ref(), "wb"))?;
                let writer = ipc.call_method1("new_file", (&sink, data.getattr("schema")?))?;
                writer.call_method1("write_table", (data,))?;
                writer.call_method0("close")?;
                sink.call_method0("close")?;
            }
        }
        let schema = dtype::infer_schema_for_interface(py, data, "Arrow")?;
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load a PyArrow table from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for PyArrow calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future Arrow-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when the expected Arrow artifact is missing, format is
    /// invalid, or PyArrow cannot deserialize the artifact.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let meta = self.to_rust(py)?;
        let format = meta.format;
        let absolute_path = RustDataInterface::Arrow(meta).artifact_path(path)?;
        require_local_file(&absolute_path)?;
        self.data = Some(match format {
            ArrowFormat::Parquet => py
                .import("pyarrow.parquet")?
                .call_method1("read_table", (&absolute_path,))?
                .unbind(),
            ArrowFormat::Ipc => {
                let ipc = py.import("pyarrow.ipc")?;
                let reader = ipc.call_method1("open_file", (&absolute_path,))?;
                reader.call_method0("read_all")?.unbind()
            }
        });
        Ok(())
    }
}
#[cfg(feature = "python")]
impl ParquetInterface {
    /// Save a parquet path or table-like source to the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for path detection or PyArrow calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future parquet-specific save options.
    ///
    /// # Returns
    ///
    /// File statistics for `data/data.parquet`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, a path-like source is not
    /// a local file, compression is invalid, table serialization fails, or
    /// local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let source = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source(
                "ParquetInterface.save requires a parquet path or table-like source",
            )
        })?;
        let data = source.bind(py);
        let meta = self.to_rust(py)?;
        let compression = meta.compression;
        let absolute_path = RustDataInterface::Parquet(meta).artifact_path(path)?;
        ensure_parent_dir(&absolute_path)?;
        if dtype::is_path_like(py, data)? {
            let source_path = dtype::extract_pathbuf(data)?;
            require_local_file(&source_path)?;
            fs::copy(&source_path, &absolute_path)?;
        } else {
            let kwargs = parquet_compression_kwargs(py, compression)?;
            let parquet = py.import("pyarrow.parquet")?;
            parquet.call_method("write_table", (data, &absolute_path), Some(&kwargs))?;
        }
        let schema = optional_schema_for_interface(py, data, "Parquet");
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load a PyArrow table from the local parquet artifact.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for PyArrow calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future parquet-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when `data/data.parquet` is missing or PyArrow cannot
    /// read the parquet artifact.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let absolute_path = RustDataInterface::Parquet(self.to_rust(py)?).artifact_path(path)?;
        require_local_file(&absolute_path)?;
        self.data = Some(
            py.import("pyarrow.parquet")?
                .call_method1("read_table", (&absolute_path,))?
                .unbind(),
        );
        Ok(())
    }
}

#[cfg(feature = "python")]
impl NumpyInterface {
    /// Save the held NumPy array to the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for NumPy calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future NumPy-specific save options.
    ///
    /// # Returns
    ///
    /// File statistics for `data/data.npy` or `data/data.npz`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, the object is not a NumPy
    /// ndarray, format is invalid, NumPy serialization fails, schema inference
    /// fails, or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let source = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source("NumpyInterface.save requires a numpy.ndarray")
        })?;
        let data = source.bind(py);
        dtype::ensure_numpy_array(py, data)?;
        let meta = self.to_rust(py)?;
        let format = meta.format;
        let absolute_path = RustDataInterface::Numpy(meta).artifact_path(path)?;
        ensure_parent_dir(&absolute_path)?;
        let numpy = py.import("numpy")?;
        match format {
            NumpyFormat::Npy => {
                let kwargs = numpy_no_pickle_kwargs(py)?;
                numpy.call_method("save", (&absolute_path, data), Some(&kwargs))?;
            }
            NumpyFormat::Npz => {
                let kwargs = numpy_npz_value_kwargs(py, data)?;
                numpy.call_method("savez", (&absolute_path,), Some(&kwargs))?;
            }
        }
        let schema = dtype::infer_schema_for_interface(py, data, "Numpy")?;
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load a NumPy array from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for NumPy calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future NumPy-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when the expected NumPy artifact is missing, format is
    /// invalid, or NumPy cannot deserialize the artifact.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let meta = self.to_rust(py)?;
        let format = meta.format;
        let absolute_path = RustDataInterface::Numpy(meta).artifact_path(path)?;
        require_local_file(&absolute_path)?;
        let numpy = py.import("numpy")?;
        let kwargs = numpy_no_pickle_kwargs(py)?;
        self.data = Some(match format {
            NumpyFormat::Npy => numpy
                .call_method("load", (&absolute_path,), Some(&kwargs))?
                .unbind(),
            NumpyFormat::Npz => {
                let loaded = numpy.call_method("load", (&absolute_path,), Some(&kwargs))?;
                loaded.get_item("value")?.unbind()
            }
        });
        Ok(())
    }
}

#[cfg(feature = "python")]
impl TorchInterface {
    /// Save the held Torch tensor or mapping to the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for Torch or safetensors calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future Torch-specific save options.
    ///
    /// # Returns
    ///
    /// File statistics for `data/data.safetensors` or `data/data.pt`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, the object is not a Torch
    /// tensor or tensor mapping, save format is invalid, serialization fails,
    /// or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let source = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source(
                "TorchInterface.save requires a torch.Tensor or tensor mapping",
            )
        })?;
        let data = source.bind(py);
        dtype::ensure_torch_tensor_or_mapping(py, data)?;
        let meta = self.to_rust(py)?;
        let save_format = meta.save_format;
        let absolute_path = RustDataInterface::Torch(meta).artifact_path(path)?;
        ensure_parent_dir(&absolute_path)?;
        match save_format {
            TorchSaveFormat::Safetensors => {
                py.import("safetensors.torch")?.call_method1(
                    "save_file",
                    (torch_to_safetensor_map(py, data)?, &absolute_path),
                )?;
            }
            TorchSaveFormat::Pickle => {
                py.import("torch")?
                    .call_method1("save", (data, &absolute_path))?;
            }
        }
        let schema = optional_schema_for_interface(py, data, "Torch");
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load a Torch tensor or mapping from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for Torch or safetensors calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future Torch-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when the expected Torch artifact is missing, save
    /// format is invalid, or framework deserialization fails.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let meta = self.to_rust(py)?;
        let save_format = meta.save_format;
        let absolute_path = RustDataInterface::Torch(meta).artifact_path(path)?;
        require_local_file(&absolute_path)?;
        self.data = Some(match save_format {
            TorchSaveFormat::Safetensors => py
                .import("safetensors.torch")?
                .call_method1("load_file", (&absolute_path,))?
                .unbind(),
            TorchSaveFormat::Pickle => {
                let kwargs = torch_weights_only_kwargs(py)?;
                py.import("torch")?
                    .call_method("load", (&absolute_path,), Some(&kwargs))?
                    .unbind()
            }
        });
        Ok(())
    }
}

#[cfg(feature = "python")]
impl SqlInterface {
    /// Save the SQL query bundle to the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for JSON conversion.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future SQL-specific save options.
    ///
    /// # Returns
    ///
    /// File statistics for `data/sql.json`.
    ///
    /// # Errors
    ///
    /// Returns an error when the SQL payload cannot be converted to Wyrd SQL
    /// logic, JSON writing fails, or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let query_bundle = sql_logic_from_data(py, self.data.as_ref())?;
        let absolute_path = RustDataInterface::Sql(self.to_rust(py)?).artifact_path(path)?;
        write_json_sorted(&absolute_path, &query_bundle)?;
        let schema = DataSchema::empty();
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load the SQL query bundle from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for JSON conversion.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future SQL-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when `data/sql.json` is missing or cannot be parsed as
    /// JSON.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let absolute_path = RustDataInterface::Sql(self.to_rust(py)?).artifact_path(path)?;
        require_local_file(&absolute_path)?;
        self.data = Some(serde_json_file_to_py(py, &absolute_path)?.unbind());
        Ok(())
    }
}

#[cfg(feature = "python")]
impl JsonlInterface {
    /// Save JSON Lines data to the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for JSON conversion.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future JSONL-specific save options.
    ///
    /// # Returns
    ///
    /// File statistics for the configured JSONL artifact path.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, compression is invalid,
    /// JSONL normalization fails, schema inference fails, or local stats cannot
    /// be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let source = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source(
                "JsonlInterface.save requires a path, iterable of dicts, or file-like object",
            )
        })?;
        let data = source.bind(py);
        let meta = self.to_rust(py)?;
        let compression = meta.compression;
        let absolute_path = RustDataInterface::Jsonl(meta).artifact_path(path)?;
        ensure_parent_dir(&absolute_path)?;
        write_jsonl_normalized(py, data.clone(), &absolute_path, compression)?;
        let schema = optional_schema_for_interface(py, data, "Jsonl");
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load JSON Lines data from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for JSON conversion.
    /// * `path` - Local DataCard materialization root.
    /// * `load_kwargs` - Optional loader-specific keyword arguments.
    ///
    /// # Errors
    ///
    /// Returns an error when the configured JSONL artifact is missing,
    /// compression is invalid, or JSONL decoding fails.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let meta = self.to_rust(py)?;
        let compression = meta.compression;
        let absolute_path = RustDataInterface::Jsonl(meta).artifact_path(path)?;
        require_local_file(&absolute_path)?;
        self.data = Some(
            read_jsonl_to_py(
                py,
                &absolute_path,
                Some(jsonl_compression_token(compression)),
                load_kwargs,
            )?
            .unbind(),
        );
        Ok(())
    }
}

#[cfg(feature = "python")]
impl ImageInterface {
    /// Save the image manifest, optionally copying referenced bytes.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for manifest conversion.
    /// * `path` - Local DataCard materialization root.
    /// * `save_kwargs` - Optional keyword arguments. `copy_bytes=true` copies
    ///   referenced files into `data/images`.
    ///
    /// # Returns
    ///
    /// File statistics for `data/manifest.json`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, format or color mode is
    /// invalid, manifest creation fails, byte copying fails, JSON writing
    /// fails, or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let source = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source(
                "ImageInterface.save requires a directory, paths, or manifest",
            )
        })?;
        let data = source.bind(py);
        let copy_bytes = bool_kwarg(save_kwargs, "copy_bytes")?;
        let meta = self.to_rust(py)?;
        let manifest = image_manifest_from_data(py, data.clone(), meta.format, meta.color_mode)?;
        if copy_bytes {
            copy_manifest_entries(&manifest, &path.join("data/images"))?;
        }
        let absolute_path = RustDataInterface::Image(meta).artifact_path(path)?;
        write_json_sorted(&absolute_path, &manifest)?;
        let schema = image_manifest_schema();
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load the image manifest from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for JSON conversion.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future image-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when `data/manifest.json` is missing or cannot be
    /// parsed as JSON.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let absolute_path = RustDataInterface::Image(self.to_rust(py)?).artifact_path(path)?;
        require_local_file(&absolute_path)?;
        self.data = Some(manifest_json_to_py(py, &absolute_path)?.unbind());
        Ok(())
    }
}

#[cfg(feature = "python")]
impl TextInterface {
    /// Save the text manifest, optionally copying referenced bytes.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for manifest conversion.
    /// * `path` - Local DataCard materialization root.
    /// * `save_kwargs` - Optional keyword arguments. `copy_bytes=true` copies
    ///   referenced files into `data/files`.
    ///
    /// # Returns
    ///
    /// File statistics for `data/manifest.json`.
    ///
    /// # Errors
    ///
    /// Returns an error when source data is missing, manifest creation fails,
    /// byte copying fails, JSON writing fails, or local stats cannot be
    /// computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let source = self.data.as_ref().ok_or_else(|| {
            WyrdPyError::missing_data_source(
                "TextInterface.save requires a directory, paths, or manifest",
            )
        })?;
        let data = source.bind(py);
        let copy_bytes = bool_kwarg(save_kwargs, "copy_bytes")?;
        let meta = self.to_rust(py)?;
        let manifest = text_manifest_from_data(py, data.clone(), &meta.encoding)?;
        if copy_bytes {
            copy_manifest_entries(&manifest, &path.join("data/files"))?;
        }
        let absolute_path = RustDataInterface::Text(meta).artifact_path(path)?;
        write_json_sorted(&absolute_path, &manifest)?;
        let schema = text_manifest_schema();
        data_stats_for_file(&absolute_path, Some(&schema))
    }

    /// Load the text manifest from the local artifact layout.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for JSON conversion.
    /// * `path` - Local DataCard materialization root.
    /// * `_load_kwargs` - Reserved for future text-specific load options.
    ///
    /// # Errors
    ///
    /// Returns an error when `data/manifest.json` is missing or cannot be
    /// parsed as JSON.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        _load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let absolute_path = RustDataInterface::Text(self.to_rust(py)?).artifact_path(path)?;
        require_local_file(&absolute_path)?;
        self.data = Some(manifest_json_to_py(py, &absolute_path)?.unbind());
        Ok(())
    }
}

#[cfg(feature = "python")]
impl HuggingfaceInterface {
    /// Save a local Hugging Face dataset or pinned remote pointer.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for dataset calls.
    /// * `path` - Local DataCard materialization root.
    /// * `_save_kwargs` - Reserved for future Hugging Face-specific save
    ///   options.
    ///
    /// # Returns
    ///
    /// Path statistics for `data/dataset` or `data/dataset_pointer.json`.
    ///
    /// # Errors
    ///
    /// Returns an error when pointer-only save lacks a pinned revision, local
    /// dataset serialization fails, pointer JSON writing fails, schema
    /// inference fails, or local stats cannot be computed.
    pub fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        _save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<DataStats> {
        let meta = self.to_rust(py)?;
        let (artifact_path, schema) = if let Some(source) = self.data.as_ref() {
            let data = source.bind(py);
            let dataset_path = path.join("data/dataset");
            fs::create_dir_all(path.join("data"))?;
            data.call_method1("save_to_disk", (&dataset_path,))?;
            let schema = optional_schema_for_interface(py, data, "Huggingface");
            (dataset_path, schema)
        } else {
            let revision = meta.revision.as_ref().ok_or_else(|| {
                WyrdPyError::validation("Huggingface pointer-only save requires a pinned revision")
            })?;
            let absolute_path = RustDataInterface::Huggingface(meta.clone()).artifact_path(path)?;
            write_json_sorted(
                &absolute_path,
                &huggingface_pointer(
                    &meta.dataset_id,
                    revision,
                    meta.split.as_deref(),
                    meta.config.as_deref(),
                ),
            )?;
            (absolute_path, DataSchema::empty())
        };
        data_stats_for_path(&artifact_path, Some(&schema))
    }

    /// Load a local Hugging Face dataset or caller-approved pinned remote pointer.
    ///
    /// # Arguments
    ///
    /// * `py` - Active Python token used for `datasets` calls.
    /// * `path` - Local DataCard materialization root.
    /// * `load_kwargs` - Optional keyword arguments. Remote pointer loading
    ///   requires `allow_remote=true`.
    ///
    /// # Errors
    ///
    /// Returns an error when neither a local dataset nor pointer exists, remote
    /// loading is not explicitly allowed, pointer parsing fails, or the
    /// `datasets` package cannot load the dataset.
    pub fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> CardPyResult<()> {
        let datasets = py.import("datasets")?;
        let pointer_path = path.join("data/dataset_pointer.json");
        let dataset_dir = path.join("data/dataset");
        self.data = Some(if pointer_path.exists() {
            let allow_remote = bool_kwarg(load_kwargs, "allow_remote")?;
            if !allow_remote {
                return Err(WyrdPyError::validation(
                    "Remote HuggingFace load requires load_kwargs.allow_remote=true",
                ));
            }
            let pointer = read_huggingface_pointer(&pointer_path)?;
            let kwargs = pointer_to_kwargs(py, pointer)?;
            datasets
                .call_method("load_dataset", (), Some(&kwargs))?
                .unbind()
        } else {
            require_local_path(&dataset_dir)?;
            datasets
                .call_method1("load_from_disk", (&dataset_dir,))?
                .unbind()
        });
        Ok(())
    }
}
