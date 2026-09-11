//! Local model artifact materialization for built-in model interfaces.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyType};
use wyrd_spec::card::model::{HuggingFaceTask, TfSaveFormat, TorchSaveFormat};

use crate::model::interfaces::helpers::{ensure_extras, qualname_of, required};
use crate::model::interfaces::kinds::{
    CatboostInterface, HuggingfaceInterface, LightgbmInterface, LightningInterface,
    SklearnInterface, TensorflowInterface, TorchInterface, XgboostInterface,
};
#[cfg(feature = "python")]
use wyrd_utils::py::WyrdPyResult;

impl SklearnInterface {
    /// Save the held sklearn model and optional preprocessor with joblib.
    ///
    /// # Errors
    /// Returns a Wyrd error when `model` is absent, required packages are not
    /// importable, or local serialization fails.
    pub(super) fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<()> {
        let _ = save_kwargs;
        save_joblib_model(
            py,
            path,
            self.model.as_deref(),
            self.preprocessor.as_deref(),
            required::SKLEARN,
            "SklearnInterface",
        )
    }

    /// Load a sklearn model and optional preprocessor from local joblib files.
    ///
    /// # Errors
    /// Returns a Wyrd error when required packages are not importable or local
    /// deserialization fails.
    pub(super) fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<()> {
        let _ = load_kwargs;
        load_joblib_model(
            py,
            path,
            &mut self.model,
            &mut self.preprocessor,
            required::SKLEARN,
        )
    }
}

impl XgboostInterface {
    /// Save the held `XGBoost` model and optional preprocessor with joblib.
    ///
    /// # Errors
    /// Returns a Wyrd error when `model` is absent, required packages are not
    /// importable, or local serialization fails.
    pub(super) fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<()> {
        let _ = save_kwargs;
        save_joblib_model(
            py,
            path,
            self.model.as_deref(),
            self.preprocessor.as_deref(),
            required::XGBOOST,
            "XgboostInterface",
        )
    }

    /// Load an `XGBoost` model and optional preprocessor from local joblib files.
    ///
    /// # Errors
    /// Returns a Wyrd error when required packages are not importable or local
    /// deserialization fails.
    pub(super) fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<()> {
        let _ = load_kwargs;
        load_joblib_model(
            py,
            path,
            &mut self.model,
            &mut self.preprocessor,
            required::XGBOOST,
        )
    }
}

impl LightgbmInterface {
    /// Save the held `LightGBM` model and optional preprocessor with joblib.
    ///
    /// # Errors
    /// Returns a Wyrd error when `model` is absent, required packages are not
    /// importable, or local serialization fails.
    pub(super) fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<()> {
        let _ = save_kwargs;
        save_joblib_model(
            py,
            path,
            self.model.as_deref(),
            self.preprocessor.as_deref(),
            required::LIGHTGBM,
            "LightgbmInterface",
        )
    }

    /// Load a `LightGBM` model and optional preprocessor from local joblib files.
    ///
    /// # Errors
    /// Returns a Wyrd error when required packages are not importable or local
    /// deserialization fails.
    pub(super) fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<()> {
        let _ = load_kwargs;
        load_joblib_model(
            py,
            path,
            &mut self.model,
            &mut self.preprocessor,
            required::LIGHTGBM,
        )
    }
}

impl CatboostInterface {
    /// Save the held `CatBoost` model and optional preprocessor with joblib.
    ///
    /// # Errors
    /// Returns a Wyrd error when `model` is absent, required packages are not
    /// importable, or local serialization fails.
    pub(super) fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<()> {
        let _ = save_kwargs;
        save_joblib_model(
            py,
            path,
            self.model.as_deref(),
            self.preprocessor.as_deref(),
            required::CATBOOST,
            "CatboostInterface",
        )
    }

    /// Load a `CatBoost` model and optional preprocessor from local joblib files.
    ///
    /// # Errors
    /// Returns a Wyrd error when required packages are not importable or local
    /// deserialization fails.
    pub(super) fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<()> {
        let _ = load_kwargs;
        load_joblib_model(
            py,
            path,
            &mut self.model,
            &mut self.preprocessor,
            required::CATBOOST,
        )
    }
}

impl TorchInterface {
    /// Save the held torch model and optional preprocessor.
    ///
    /// # Errors
    /// Returns a Wyrd error when `model` is absent, required packages are not
    /// importable, or local serialization fails.
    pub(super) fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<()> {
        let _ = save_kwargs;
        let model = require_model(self.model.as_deref(), "TorchInterface")?;
        fs::create_dir_all(path)?;
        match self.save_format {
            TorchSaveFormat::Safetensors => {
                ensure_extras_pair(py, required::TORCH_SAFETENSORS)?;
                let state_dict = model.bind(py).call_method0("state_dict")?;
                py.import("safetensors.torch")?
                    .call_method1("save_file", (state_dict, &path.join("model.safetensors")))?;
            }
            TorchSaveFormat::Pickle => {
                ensure_extras_pair(py, required::TORCH_PICKLE)?;
                let state_dict = model.bind(py).call_method0("state_dict")?;
                py.import("torch")?
                    .call_method1("save", (state_dict, &path.join("model.pt")))?;
            }
        }
        save_preprocessor_joblib(py, path, self.preprocessor.as_deref(), "torch")
    }

    /// Load a torch model from the local artifact layout.
    ///
    /// # Errors
    /// Returns a Wyrd error when required packages are not importable, a
    /// safetensors load has no attached module instance, or deserialization
    /// fails.
    pub(super) fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<()> {
        let _ = load_kwargs;
        match self.save_format {
            TorchSaveFormat::Safetensors => {
                ensure_extras_pair(py, required::TORCH_SAFETENSORS)?;
                let model = self.model.as_deref().ok_or_else(|| {
                    crate::error::model_validation(
                        "TorchInterface.load with safetensors requires an attached torch module instance",
                    )
                })?;
                let state_dict = py
                    .import("safetensors.torch")?
                    .call_method1("load_file", (&path.join("model.safetensors"),))?;
                model
                    .bind(py)
                    .call_method1("load_state_dict", (state_dict,))?;
            }
            TorchSaveFormat::Pickle => {
                ensure_extras_pair(py, required::TORCH_PICKLE)?;
                let kwargs = PyDict::new(py);
                kwargs.set_item("map_location", "cpu")?;
                kwargs.set_item("weights_only", true)?;
                let model = py.import("torch")?.call_method(
                    "load",
                    (&path.join("model.pt"),),
                    Some(&kwargs),
                )?;
                self.model = Some(Arc::new(model.unbind()));
            }
        }
        self.preprocessor = load_preprocessor_joblib(py, path, "torch")?;
        Ok(())
    }
}

impl LightningInterface {
    /// Save the held Lightning module checkpoint and optional preprocessor.
    ///
    /// # Errors
    /// Returns a Wyrd error when no model is attached, required packages are
    /// not importable, or checkpoint serialization fails.
    pub(super) fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<()> {
        let _ = save_kwargs;
        let model = require_model(self.model.as_deref(), "LightningInterface")?;
        ensure_extras_pair(py, required::LIGHTNING)?;
        fs::create_dir_all(path)?;
        let checkpoint_path = path.join("model.ckpt");
        if let Some(trainer) = self.trainer.as_deref() {
            trainer
                .bind(py)
                .call_method1("save_checkpoint", (&checkpoint_path,))?;
        } else {
            let state_dict = model.bind(py).call_method0("state_dict")?;
            let checkpoint = PyDict::new(py);
            checkpoint.set_item("state_dict", state_dict)?;
            py.import("torch")?
                .call_method1("save", (checkpoint, &checkpoint_path))?;
        }
        save_preprocessor_joblib(py, path, self.preprocessor.as_deref(), "lightning")
    }

    /// Load a Lightning module checkpoint from the local artifact layout.
    ///
    /// # Errors
    /// Returns a Wyrd error when no module class or instance is attached,
    /// required packages are unavailable, or checkpoint loading fails.
    pub(super) fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<()> {
        let _ = load_kwargs;
        ensure_extras_pair(py, required::LIGHTNING)?;
        let checkpoint_path = path.join("model.ckpt");
        let model = self.model.as_deref().ok_or_else(|| {
            crate::error::model_validation(
                "LightningInterface.load requires an attached LightningModule class or instance",
            )
        })?;
        let bound = model.bind(py);
        if bound.cast::<PyType>().is_ok() {
            let loaded = bound.call_method1("load_from_checkpoint", (&checkpoint_path,))?;
            self.model = Some(Arc::new(loaded.unbind()));
        } else {
            let kwargs = PyDict::new(py);
            kwargs.set_item("map_location", "cpu")?;
            let checkpoint =
                py.import("torch")?
                    .call_method("load", (&checkpoint_path,), Some(&kwargs))?;
            let state_dict = checkpoint.get_item("state_dict")?;
            bound.call_method1("load_state_dict", (state_dict,))?;
        }
        self.preprocessor = load_preprocessor_joblib(py, path, "lightning")?;
        Ok(())
    }
}

impl TensorflowInterface {
    /// Save the held TensorFlow/Keras model and optional preprocessor.
    ///
    /// # Errors
    /// Returns a Wyrd error when no model is attached, required packages are
    /// unavailable, or serialization fails.
    pub(super) fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<()> {
        let _ = save_kwargs;
        let model = require_model(self.model.as_deref(), "TensorflowInterface")?;
        ensure_extras_pair(py, required::TENSORFLOW)?;
        fs::create_dir_all(path)?;
        match self.save_format {
            TfSaveFormat::Keras => {
                model
                    .bind(py)
                    .call_method1("save", (&path.join("model.keras"),))?;
            }
            TfSaveFormat::SavedModel => {
                model
                    .bind(py)
                    .call_method1("export", (&path.join("savedmodel"),))?;
            }
        }
        save_preprocessor_joblib(py, path, self.preprocessor.as_deref(), "tensorflow")
    }

    /// Load a TensorFlow/Keras model from the local artifact layout.
    ///
    /// # Errors
    /// Returns a Wyrd error when required packages are unavailable or local
    /// deserialization fails.
    pub(super) fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<()> {
        let _ = load_kwargs;
        ensure_extras_pair(py, required::TENSORFLOW)?;
        let tf = py.import("tensorflow")?;
        let model = match self.save_format {
            TfSaveFormat::Keras => tf
                .getattr("keras")?
                .getattr("models")?
                .call_method1("load_model", (&path.join("model.keras"),))?,
            TfSaveFormat::SavedModel => tf
                .getattr("saved_model")?
                .call_method1("load", (&path.join("savedmodel"),))?,
        };
        self.model = Some(Arc::new(model.unbind()));
        self.preprocessor = load_preprocessor_joblib(py, path, "tensorflow")?;
        Ok(())
    }
}

impl HuggingfaceInterface {
    /// Save the held Hugging Face model and optional processor with `save_pretrained`.
    ///
    /// # Errors
    /// Returns a Wyrd error when no model is attached, required packages are
    /// unavailable, or Hugging Face serialization fails.
    pub(super) fn save_inner(
        &self,
        py: Python<'_>,
        path: &Path,
        save_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<()> {
        let _ = save_kwargs;
        let model = require_model(self.model.as_deref(), "HuggingfaceInterface")?;
        ensure_extras_pair(py, required::HUGGINGFACE)?;
        let model_dir = path.join("model");
        fs::create_dir_all(&model_dir)?;
        model
            .bind(py)
            .call_method1("save_pretrained", (&model_dir,))?;
        if let Some(processor) = self.processor.as_deref() {
            processor
                .bind(py)
                .call_method1("save_pretrained", (&model_dir,))?;
        }
        Ok(())
    }

    /// Load a Hugging Face model and optional processor from `path/model`.
    ///
    /// # Errors
    /// Returns a Wyrd error when required packages are unavailable or local
    /// deserialization fails.
    pub(super) fn load_inner(
        &mut self,
        py: Python<'_>,
        path: &Path,
        load_kwargs: Option<&Bound<'_, PyDict>>,
    ) -> WyrdPyResult<()> {
        let _ = load_kwargs;
        ensure_extras_pair(py, required::HUGGINGFACE)?;
        let transformers = py.import("transformers")?;
        let model_dir = path.join("model");
        let auto_model_class = auto_model_class_for(self.hf_task);
        let model = transformers
            .getattr(auto_model_class)
            .map_err(|_| {
                crate::error::model_validation(format!(
                    "transformers.{auto_model_class} is not available for this Hugging Face task"
                ))
            })?
            .call_method1("from_pretrained", (&model_dir,))?;
        self.model_subtype = Some(qualname_of(py, &model)?);
        self.model = Some(Arc::new(model.unbind()));
        self.processor = load_huggingface_processor(&transformers, &model_dir);
        Ok(())
    }
}

fn save_joblib_model(
    py: Python<'_>,
    path: &Path,
    model: Option<&Py<PyAny>>,
    preprocessor: Option<&Py<PyAny>>,
    required: (&str, &[&str]),
    interface_name: &str,
) -> WyrdPyResult<()> {
    let model = require_model(model, interface_name)?;
    ensure_extras_pair(py, required)?;
    fs::create_dir_all(path)?;
    let joblib = py.import("joblib")?;
    joblib.call_method1("dump", (model.bind(py), &path.join("model.joblib")))?;
    if let Some(preprocessor) = preprocessor {
        joblib.call_method1(
            "dump",
            (preprocessor.bind(py), &path.join("preprocessor.joblib")),
        )?;
    }
    Ok(())
}

fn load_joblib_model(
    py: Python<'_>,
    path: &Path,
    model: &mut Option<Arc<Py<PyAny>>>,
    preprocessor: &mut Option<Arc<Py<PyAny>>>,
    required: (&str, &[&str]),
) -> WyrdPyResult<()> {
    ensure_extras_pair(py, required)?;
    let joblib = py.import("joblib")?;
    let loaded = joblib.call_method1("load", (&path.join("model.joblib"),))?;
    if loaded.is_none() {
        return Err(crate::error::model_validation("joblib model artifact loaded as None").into());
    }
    *model = Some(Arc::new(loaded.unbind()));
    *preprocessor = load_preprocessor_joblib(py, path, required.0)?;
    Ok(())
}

fn save_preprocessor_joblib(
    py: Python<'_>,
    path: &Path,
    preprocessor: Option<&Py<PyAny>>,
    extras: &str,
) -> WyrdPyResult<()> {
    if let Some(preprocessor) = preprocessor {
        ensure_extras(py, extras, &["joblib"])?;
        py.import("joblib")?.call_method1(
            "dump",
            (preprocessor.bind(py), &path.join("preprocessor.joblib")),
        )?;
    }
    Ok(())
}

fn load_preprocessor_joblib(
    py: Python<'_>,
    path: &Path,
    extras: &str,
) -> WyrdPyResult<Option<Arc<Py<PyAny>>>> {
    let preprocessor_path = path.join("preprocessor.joblib");
    if !preprocessor_path.exists() {
        return Ok(None);
    }
    ensure_extras(py, extras, &["joblib"])?;
    Ok(Some(Arc::new(
        py.import("joblib")?
            .call_method1("load", (&preprocessor_path,))?
            .unbind(),
    )))
}

fn require_model<'a>(
    model: Option<&'a Py<PyAny>>,
    interface_name: &str,
) -> WyrdPyResult<&'a Py<PyAny>> {
    model
        .ok_or_else(|| {
            crate::error::model_validation(format!(
                "{interface_name}.save requires `model` to be set"
            ))
        })
        .map_err(Into::into)
}

fn ensure_extras_pair(py: Python<'_>, required: (&str, &[&str])) -> WyrdPyResult<()> {
    ensure_extras(py, required.0, required.1)
}

fn load_huggingface_processor(
    transformers: &Bound<'_, PyAny>,
    model_dir: &Path,
) -> Option<Arc<Py<PyAny>>> {
    if let Ok(auto_processor) = transformers.getattr("AutoProcessor")
        && let Ok(processor) = auto_processor.call_method1("from_pretrained", (model_dir,))
    {
        return Some(Arc::new(processor.unbind()));
    }
    if let Ok(auto_tokenizer) = transformers.getattr("AutoTokenizer")
        && let Ok(tokenizer) = auto_tokenizer.call_method1("from_pretrained", (model_dir,))
    {
        return Some(Arc::new(tokenizer.unbind()));
    }
    None
}

fn auto_model_class_for(task: HuggingFaceTask) -> &'static str {
    match task {
        HuggingFaceTask::TextClassification | HuggingFaceTask::ZeroShotClassification => {
            "AutoModelForSequenceClassification"
        }
        HuggingFaceTask::TokenClassification => "AutoModelForTokenClassification",
        HuggingFaceTask::QuestionAnswering => "AutoModelForQuestionAnswering",
        HuggingFaceTask::Summarization | HuggingFaceTask::Translation => "AutoModelForSeq2SeqLM",
        HuggingFaceTask::TextGeneration | HuggingFaceTask::Conversational => "AutoModelForCausalLM",
        HuggingFaceTask::FillMask => "AutoModelForMaskedLM",
        HuggingFaceTask::ImageClassification => "AutoModelForImageClassification",
        HuggingFaceTask::ObjectDetection => "AutoModelForObjectDetection",
        HuggingFaceTask::ImageSegmentation => "AutoModelForImageSegmentation",
        HuggingFaceTask::ImageToText => "AutoModelForVision2Seq",
        HuggingFaceTask::ImageToImage => "AutoModelForImageToImage",
        HuggingFaceTask::DepthEstimation => "AutoModelForDepthEstimation",
        HuggingFaceTask::AudioClassification => "AutoModelForAudioClassification",
        HuggingFaceTask::AutomaticSpeechRecognition => "AutoModelForSpeechSeq2Seq",
        HuggingFaceTask::TextToSpeech => "AutoModelForTextToSpectrogram",
        HuggingFaceTask::DocumentQuestionAnswering => "AutoModelForDocumentQuestionAnswering",
        HuggingFaceTask::VisualQuestionAnswering => "AutoModelForVisualQuestionAnswering",
        HuggingFaceTask::TableQuestionAnswering => "AutoModelForTableQuestionAnswering",
        HuggingFaceTask::MultipleChoice => "AutoModelForMultipleChoice",
        HuggingFaceTask::TextToImage
        | HuggingFaceTask::AudioToAudio
        | HuggingFaceTask::TabularClassification
        | HuggingFaceTask::TabularRegression
        | HuggingFaceTask::FeatureExtraction
        | HuggingFaceTask::SentenceSimilarity
        | HuggingFaceTask::Embedding
        | HuggingFaceTask::Other => "AutoModel",
    }
}

#[cfg(test)]
mod tests {
    use super::auto_model_class_for;
    use wyrd_spec::card::model::HuggingFaceTask;

    #[test]
    fn maps_huggingface_tasks_to_auto_model_classes() {
        assert_eq!(
            auto_model_class_for(HuggingFaceTask::TextClassification),
            "AutoModelForSequenceClassification"
        );
        assert_eq!(
            auto_model_class_for(HuggingFaceTask::Summarization),
            "AutoModelForSeq2SeqLM"
        );
        assert_eq!(
            auto_model_class_for(HuggingFaceTask::TextGeneration),
            "AutoModelForCausalLM"
        );
        assert_eq!(auto_model_class_for(HuggingFaceTask::Other), "AutoModel");
    }
}
