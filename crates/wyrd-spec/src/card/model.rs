//! Model Card spec types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::card::field::FieldSpec;
use crate::reference::CardRef;

/// Body of a Model card.
///
/// This pure contract describes which framework loads the model, which task it
/// performs, which fields it consumes and produces, and which Artifact cards
/// hold model-related bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ModelSpec {
    /// Framework interface tag and per-interface config metadata.
    pub interface: ModelInterface,
    /// Closed Wyrd ML task taxonomy.
    pub task_type: TaskType,
    /// Typed input and output schema.
    pub signature: ModelSignature,
    /// Optional canonical sample input description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_input: Option<SampleInput>,
    /// Durable Artifact card references linked to this model card.
    #[serde(default)]
    pub artifact_refs: Vec<CardRef>,
}

impl ModelSpec {
    /// Iterate durable Artifact card references linked to this model card.
    pub fn artifact_refs(&self) -> impl Iterator<Item = &CardRef> {
        self.artifact_refs.iter()
    }
}

/// Framework interface tag for a Model spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", content = "meta")]
pub enum ModelInterface {
    /// scikit-learn estimator persisted through the sklearn loader.
    Sklearn(SklearnMeta),
    /// XGBoost model persisted through the xgboost loader.
    Xgboost(XgboostMeta),
    /// LightGBM model persisted through the lightgbm loader.
    Lightgbm(LightgbmMeta),
    /// CatBoost model persisted through the catboost loader.
    Catboost(CatboostMeta),
    /// PyTorch module persisted through the torch loader.
    Torch(TorchMeta),
    /// PyTorch Lightning module persisted as a trainer checkpoint.
    Lightning(LightningMeta),
    /// TensorFlow or Keras model persisted through the tensorflow loader.
    Tensorflow(TensorflowMeta),
    /// Hugging Face model persisted through a local snapshot or pinned repo.
    Huggingface(HuggingfaceMeta),
    /// User-supplied loader metadata.
    Custom(CustomMeta),
}

/// Config for the Sklearn model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SklearnMeta {
    /// Installed sklearn version captured for reproducibility.
    pub framework_version: String,
    /// Concrete estimator class name, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
}

/// Config for the Xgboost model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct XgboostMeta {
    /// Installed xgboost version captured for reproducibility.
    pub framework_version: String,
    /// Concrete model subtype, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
}

/// Config for the Lightgbm model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct LightgbmMeta {
    /// Installed lightgbm version captured for reproducibility.
    pub framework_version: String,
    /// Concrete model subtype, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
}

/// Config for the Catboost model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CatboostMeta {
    /// Installed catboost version captured for reproducibility.
    pub framework_version: String,
    /// Concrete model subtype, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
}

/// Config for the Torch model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TorchMeta {
    /// Installed torch version captured for reproducibility.
    pub framework_version: String,
    /// Concrete module class name, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
    /// Save format for the torch artifact.
    pub save_format: TorchSaveFormat,
}

/// Config for the Lightning model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct LightningMeta {
    /// Installed pytorch-lightning version captured for reproducibility.
    pub framework_version: String,
    /// Concrete LightningModule class name, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
}

/// Config for the Tensorflow model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TensorflowMeta {
    /// Installed tensorflow or keras version captured for reproducibility.
    pub framework_version: String,
    /// Concrete model class name, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
    /// Save format for the tensorflow artifact.
    pub save_format: TfSaveFormat,
}

/// Config for the Huggingface model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct HuggingfaceMeta {
    /// Installed transformers or diffusers version captured for reproducibility.
    pub framework_version: String,
    /// Concrete model class name, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
    /// Hugging Face task identifier required for rehydration.
    pub hf_task: HuggingFaceTask,
    /// Optional Hugging Face Hub repo id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    /// Optional pinned revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

/// Config for the Custom model interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CustomMeta {
    /// Reported framework version of the user-supplied loader environment.
    pub framework_version: String,
    /// Optional informational subtype string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_subtype: Option<String>,
    /// Python module path containing the user-supplied loader.
    pub loader_module: String,
    /// Loader class or callable name.
    pub loader_class: String,
    /// Free-form string metadata that does not change the Wyrd contract.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}

/// Closed Wyrd ML task taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum TaskType {
    /// Two-class classification.
    BinaryClassification,
    /// Classification with more than two classes.
    MultiClassClassification,
    /// Continuous-valued regression.
    Regression,
    /// Unsupervised clustering.
    Clustering,
    /// Anomaly or outlier detection.
    AnomalyDetection,
    /// Time-series forecasting.
    Forecasting,
    /// Generative model task.
    Generation,
    /// Long-tail task described by interface metadata.
    Other,
}

/// Save format for the Torch model interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum TorchSaveFormat {
    /// Tensors-only safetensors state dict.
    Safetensors,
    /// Pickle-backed torch save format.
    Pickle,
}

/// Save format for the Tensorflow model interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum TfSaveFormat {
    /// Keras v3 single-file archive.
    Keras,
    /// SavedModel directory layout.
    SavedModel,
}

/// Hugging Face task identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum HuggingFaceTask {
    /// text-classification
    TextClassification,
    /// token-classification
    TokenClassification,
    /// question-answering
    QuestionAnswering,
    /// summarization
    Summarization,
    /// translation
    Translation,
    /// text-generation
    TextGeneration,
    /// fill-mask
    FillMask,
    /// zero-shot-classification
    ZeroShotClassification,
    /// image-classification
    ImageClassification,
    /// object-detection
    ObjectDetection,
    /// image-segmentation
    ImageSegmentation,
    /// image-to-text
    ImageToText,
    /// image-to-image
    ImageToImage,
    /// text-to-image
    TextToImage,
    /// depth-estimation
    DepthEstimation,
    /// audio-classification
    AudioClassification,
    /// automatic-speech-recognition
    AutomaticSpeechRecognition,
    /// audio-to-audio
    AudioToAudio,
    /// text-to-speech
    TextToSpeech,
    /// tabular-classification
    TabularClassification,
    /// tabular-regression
    TabularRegression,
    /// feature-extraction
    FeatureExtraction,
    /// sentence-similarity
    SentenceSimilarity,
    /// conversational
    Conversational,
    /// document-question-answering
    DocumentQuestionAnswering,
    /// visual-question-answering
    VisualQuestionAnswering,
    /// table-question-answering
    TableQuestionAnswering,
    /// embedding
    Embedding,
    /// multiple-choice
    MultipleChoice,
    /// Long-tail task identifier not yet first-class.
    Other,
}

/// Typed input and output schema for a Model spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ModelSignature {
    /// Ordered input field schema.
    pub inputs: Vec<FieldSpec>,
    /// Ordered output field schema.
    pub outputs: Vec<FieldSpec>,
}

/// Sample input description for loader-side rehydration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SampleInput {
    /// Closed kind tag selecting the loader-side deserializer.
    pub kind: SampleInputKind,
}

/// Closed set of sample input shapes supported by Wyrd at the contract layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum SampleInputKind {
    /// pandas dataframe serialized as parquet.
    Pandas,
    /// polars dataframe serialized as parquet.
    Polars,
    /// pyarrow table or record batch serialized as IPC.
    Arrow,
    /// numpy array serialized as npz.
    Numpy,
    /// torch tensor dict serialized as safetensors.
    Torch,
    /// TensorFlow tensor dict serialized as npz.
    Tf,
    /// JSON-safe Python dict serialized as json.
    Dict,
    /// JSON-safe Python list serialized as json.
    List,
    /// JSON-safe Python tuple serialized as json.
    Tuple,
    /// Plain UTF-8 string serialized as text.
    Str,
    /// No sample input declared.
    None,
}
