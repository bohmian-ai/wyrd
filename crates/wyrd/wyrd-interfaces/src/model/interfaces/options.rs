use wyrd_spec::card::model::{
    HuggingFaceTask, SampleInputKind, TaskType, TfSaveFormat, TorchSaveFormat,
};
use wyrd_spec::error::WyrdError;

/// Parse the `save_format` string option for `TorchInterface`.
///
/// # Errors
/// Returns an invalid-interface-option error for unknown tokens.
pub fn parse_torch_save_format(value: &str) -> Result<TorchSaveFormat, WyrdError> {
    match normalize_option(value).as_str() {
        "safetensors" => Ok(TorchSaveFormat::Safetensors),
        "pickle" => Ok(TorchSaveFormat::Pickle),
        got => Err(crate::error::invalid_interface_option(
            "save_format",
            got,
            ["safetensors", "pickle"],
        )),
    }
}

/// Parse the `save_format` string option for `TensorflowInterface`.
///
/// # Errors
/// Returns an invalid-interface-option error for unknown tokens.
pub fn parse_tf_save_format(value: &str) -> Result<TfSaveFormat, WyrdError> {
    match normalize_option(value).as_str() {
        "keras" => Ok(TfSaveFormat::Keras),
        "savedmodel" | "saved_model" => Ok(TfSaveFormat::SavedModel),
        got => Err(crate::error::invalid_interface_option(
            "save_format",
            got,
            ["keras", "savedmodel"],
        )),
    }
}

/// Parse a Hugging Face task token.
///
/// # Errors
/// Returns an invalid-interface-option error for unknown tokens.
pub fn parse_huggingface_task(value: &str) -> Result<HuggingFaceTask, WyrdError> {
    match normalize_option(value).as_str() {
        "text_classification" => Ok(HuggingFaceTask::TextClassification),
        "token_classification" => Ok(HuggingFaceTask::TokenClassification),
        "question_answering" => Ok(HuggingFaceTask::QuestionAnswering),
        "summarization" => Ok(HuggingFaceTask::Summarization),
        "translation" => Ok(HuggingFaceTask::Translation),
        "text_generation" => Ok(HuggingFaceTask::TextGeneration),
        "fill_mask" => Ok(HuggingFaceTask::FillMask),
        "zero_shot_classification" => Ok(HuggingFaceTask::ZeroShotClassification),
        "image_classification" => Ok(HuggingFaceTask::ImageClassification),
        "object_detection" => Ok(HuggingFaceTask::ObjectDetection),
        "image_segmentation" => Ok(HuggingFaceTask::ImageSegmentation),
        "image_to_text" => Ok(HuggingFaceTask::ImageToText),
        "image_to_image" => Ok(HuggingFaceTask::ImageToImage),
        "text_to_image" => Ok(HuggingFaceTask::TextToImage),
        "depth_estimation" => Ok(HuggingFaceTask::DepthEstimation),
        "audio_classification" => Ok(HuggingFaceTask::AudioClassification),
        "automatic_speech_recognition" => Ok(HuggingFaceTask::AutomaticSpeechRecognition),
        "audio_to_audio" => Ok(HuggingFaceTask::AudioToAudio),
        "text_to_speech" => Ok(HuggingFaceTask::TextToSpeech),
        "tabular_classification" => Ok(HuggingFaceTask::TabularClassification),
        "tabular_regression" => Ok(HuggingFaceTask::TabularRegression),
        "feature_extraction" => Ok(HuggingFaceTask::FeatureExtraction),
        "sentence_similarity" => Ok(HuggingFaceTask::SentenceSimilarity),
        "conversational" => Ok(HuggingFaceTask::Conversational),
        "document_question_answering" => Ok(HuggingFaceTask::DocumentQuestionAnswering),
        "visual_question_answering" => Ok(HuggingFaceTask::VisualQuestionAnswering),
        "table_question_answering" => Ok(HuggingFaceTask::TableQuestionAnswering),
        "embedding" => Ok(HuggingFaceTask::Embedding),
        "multiple_choice" => Ok(HuggingFaceTask::MultipleChoice),
        "other" => Ok(HuggingFaceTask::Other),
        got => Err(crate::error::invalid_interface_option(
            "hf_task",
            got,
            HUGGINGFACE_TASK_TOKENS,
        )),
    }
}

/// Parse a sample input kind token.
///
/// # Errors
/// Returns an invalid-interface-option error for unknown tokens.
pub fn parse_sample_input_kind(value: &str) -> Result<SampleInputKind, WyrdError> {
    match normalize_option(value).as_str() {
        "pandas" => Ok(SampleInputKind::Pandas),
        "polars" => Ok(SampleInputKind::Polars),
        "arrow" => Ok(SampleInputKind::Arrow),
        "numpy" => Ok(SampleInputKind::Numpy),
        "torch" => Ok(SampleInputKind::Torch),
        "tf" | "tensorflow" => Ok(SampleInputKind::Tf),
        "dict" => Ok(SampleInputKind::Dict),
        "list" => Ok(SampleInputKind::List),
        "tuple" => Ok(SampleInputKind::Tuple),
        "str" | "string" => Ok(SampleInputKind::Str),
        "none" => Ok(SampleInputKind::None),
        got => Err(crate::error::invalid_interface_option(
            "kind",
            got,
            [
                "pandas", "polars", "arrow", "numpy", "torch", "tf", "dict", "list", "tuple",
                "str", "none",
            ],
        )),
    }
}

/// Parse a model task type token.
///
/// # Errors
/// Returns an invalid-interface-option error for unknown tokens.
pub fn parse_task_type(value: &str) -> Result<TaskType, WyrdError> {
    match normalize_option(value).as_str() {
        "binary_classification" => Ok(TaskType::BinaryClassification),
        "multi_class_classification" | "multiclass_classification" => {
            Ok(TaskType::MultiClassClassification)
        }
        "regression" => Ok(TaskType::Regression),
        "clustering" => Ok(TaskType::Clustering),
        "anomaly_detection" => Ok(TaskType::AnomalyDetection),
        "forecasting" => Ok(TaskType::Forecasting),
        "generation" => Ok(TaskType::Generation),
        "other" => Ok(TaskType::Other),
        got => Err(crate::error::invalid_interface_option(
            "task_type",
            got,
            [
                "binary_classification",
                "multi_class_classification",
                "regression",
                "clustering",
                "anomaly_detection",
                "forecasting",
                "generation",
                "other",
            ],
        )),
    }
}

fn normalize_option(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('-', "_")
}

#[cfg(feature = "python")]
pub(super) fn torch_save_format_token(value: TorchSaveFormat) -> &'static str {
    match value {
        TorchSaveFormat::Safetensors => "safetensors",
        TorchSaveFormat::Pickle => "pickle",
    }
}

#[cfg(feature = "python")]
pub(super) fn tf_save_format_token(value: TfSaveFormat) -> &'static str {
    match value {
        TfSaveFormat::Keras => "keras",
        TfSaveFormat::SavedModel => "savedmodel",
    }
}

#[cfg(feature = "python")]
pub(super) fn huggingface_task_token(value: HuggingFaceTask) -> &'static str {
    match value {
        HuggingFaceTask::TextClassification => "text-classification",
        HuggingFaceTask::TokenClassification => "token-classification",
        HuggingFaceTask::QuestionAnswering => "question-answering",
        HuggingFaceTask::Summarization => "summarization",
        HuggingFaceTask::Translation => "translation",
        HuggingFaceTask::TextGeneration => "text-generation",
        HuggingFaceTask::FillMask => "fill-mask",
        HuggingFaceTask::ZeroShotClassification => "zero-shot-classification",
        HuggingFaceTask::ImageClassification => "image-classification",
        HuggingFaceTask::ObjectDetection => "object-detection",
        HuggingFaceTask::ImageSegmentation => "image-segmentation",
        HuggingFaceTask::ImageToText => "image-to-text",
        HuggingFaceTask::ImageToImage => "image-to-image",
        HuggingFaceTask::TextToImage => "text-to-image",
        HuggingFaceTask::DepthEstimation => "depth-estimation",
        HuggingFaceTask::AudioClassification => "audio-classification",
        HuggingFaceTask::AutomaticSpeechRecognition => "automatic-speech-recognition",
        HuggingFaceTask::AudioToAudio => "audio-to-audio",
        HuggingFaceTask::TextToSpeech => "text-to-speech",
        HuggingFaceTask::TabularClassification => "tabular-classification",
        HuggingFaceTask::TabularRegression => "tabular-regression",
        HuggingFaceTask::FeatureExtraction => "feature-extraction",
        HuggingFaceTask::SentenceSimilarity => "sentence-similarity",
        HuggingFaceTask::Conversational => "conversational",
        HuggingFaceTask::DocumentQuestionAnswering => "document-question-answering",
        HuggingFaceTask::VisualQuestionAnswering => "visual-question-answering",
        HuggingFaceTask::TableQuestionAnswering => "table-question-answering",
        HuggingFaceTask::Embedding => "embedding",
        HuggingFaceTask::MultipleChoice => "multiple-choice",
        HuggingFaceTask::Other => "other",
    }
}

#[cfg(feature = "python")]
pub(crate) fn sample_input_kind_token(value: SampleInputKind) -> &'static str {
    match value {
        SampleInputKind::Pandas => "pandas",
        SampleInputKind::Polars => "polars",
        SampleInputKind::Arrow => "arrow",
        SampleInputKind::Numpy => "numpy",
        SampleInputKind::Torch => "torch",
        SampleInputKind::Tf => "tf",
        SampleInputKind::Dict => "dict",
        SampleInputKind::List => "list",
        SampleInputKind::Tuple => "tuple",
        SampleInputKind::Str => "str",
        SampleInputKind::None => "none",
    }
}

const HUGGINGFACE_TASK_TOKENS: [&str; 30] = [
    "text-classification",
    "token-classification",
    "question-answering",
    "summarization",
    "translation",
    "text-generation",
    "fill-mask",
    "zero-shot-classification",
    "image-classification",
    "object-detection",
    "image-segmentation",
    "image-to-text",
    "image-to-image",
    "text-to-image",
    "depth-estimation",
    "audio-classification",
    "automatic-speech-recognition",
    "audio-to-audio",
    "text-to-speech",
    "tabular-classification",
    "tabular-regression",
    "feature-extraction",
    "sentence-similarity",
    "conversational",
    "document-question-answering",
    "visual-question-answering",
    "table-question-answering",
    "embedding",
    "multiple-choice",
    "other",
];

#[cfg(test)]
mod tests {
    use super::{
        parse_huggingface_task, parse_sample_input_kind, parse_task_type, parse_tf_save_format,
        parse_torch_save_format,
    };
    use wyrd_spec::card::model::{
        HuggingFaceTask, SampleInputKind, TaskType, TfSaveFormat, TorchSaveFormat,
    };
    use wyrd_spec::error::WyrdError;

    #[test]
    fn parses_locked_model_options() {
        assert_eq!(
            parse_torch_save_format("safetensors").expect("torch format"),
            TorchSaveFormat::Safetensors
        );
        assert_eq!(
            parse_tf_save_format("saved_model").expect("tf format"),
            TfSaveFormat::SavedModel
        );
        assert_eq!(
            parse_huggingface_task("text-classification").expect("hf task"),
            HuggingFaceTask::TextClassification
        );
        assert_eq!(
            parse_sample_input_kind("tensorflow").expect("sample kind"),
            SampleInputKind::Tf
        );
        assert_eq!(
            parse_task_type("multi-class-classification").expect("task type"),
            TaskType::MultiClassClassification
        );
    }

    #[test]
    fn rejects_unknown_model_options_with_wyrd_code() {
        assert_invalid_interface_option(
            &parse_torch_save_format("zip").expect_err("invalid torch format"),
        );
        assert_invalid_interface_option(
            &parse_tf_save_format("pb").expect_err("invalid tf format"),
        );
        assert_invalid_interface_option(
            &parse_huggingface_task("ranking").expect_err("invalid hf task"),
        );
        assert_invalid_interface_option(
            &parse_sample_input_kind("bytes").expect_err("invalid sample kind"),
        );
        assert_invalid_interface_option(
            &parse_task_type("ranking").expect_err("invalid task type"),
        );
    }

    fn assert_invalid_interface_option(error: &WyrdError) {
        assert_eq!(error.code(), "WYRD_DATA_400_INVALID_INTERFACE_OPTION");
    }
}
