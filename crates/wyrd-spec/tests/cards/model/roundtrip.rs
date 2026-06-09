use std::collections::BTreeMap;

use serde::Serialize;
use serde::de::DeserializeOwned;
use wyrd_spec::card::field::FieldSpec;
use wyrd_spec::card::model::{
    CatboostMeta, CustomMeta, HuggingFaceTask, HuggingfaceMeta, LightgbmMeta, LightningMeta,
    ModelInterface, ModelSignature, ModelSpec, SampleInput, SampleInputKind, SklearnMeta, TaskType,
    TensorflowMeta, TfSaveFormat, TorchMeta, TorchSaveFormat, XgboostMeta,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, ColumnName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::version::VersionBlock;

fn col(name: &str) -> ColumnName {
    ColumnName::new(name).unwrap()
}

fn field(name: &str, dtype: &str) -> FieldSpec {
    FieldSpec::new(col(name), dtype)
}

fn model_ref(name: &str) -> CardRef {
    CardRef {
        kind: CardKind::Artifact,
        name: CardName::new(name).unwrap(),
        version: VersionBlock::parse("1.0.0").unwrap(),
        space: None,
        uid: None,
    }
}

fn signature() -> ModelSignature {
    ModelSignature::new(vec![field("x", "float64")], vec![field("y", "bool")])
}

fn spec(interface: ModelInterface) -> ModelSpec {
    ModelSpec {
        interface,
        task_type: TaskType::Other,
        signature: signature(),
        sample_input: Some(SampleInput::new(SampleInputKind::Dict)),
        card_refs: vec![model_ref("model")],
    }
}

fn assert_json_yaml_roundtrip<T>(value: &T)
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string_pretty(value).unwrap();
    let from_json: T = serde_json::from_str(&json).unwrap();
    assert_eq!(&from_json, value);

    let yaml = serde_yaml::to_string(value).unwrap();
    let from_yaml: T = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(&from_yaml, value);
}

fn round_trip_spec(interface: ModelInterface) {
    assert_json_yaml_roundtrip(&spec(interface));
}

fn huggingface_interface(task: HuggingFaceTask) -> ModelInterface {
    ModelInterface::Huggingface(HuggingfaceMeta {
        framework_version: "4.40.0".to_string(),
        model_subtype: None,
        hf_task: task,
        repo_id: Some("acme/model".to_string()),
        revision: Some("abcdef0".to_string()),
    })
}

#[test]
fn every_model_interface_variant_round_trips_json_and_yaml() {
    let mut extra = BTreeMap::new();
    extra.insert("graph_backend".to_string(), "dgl".to_string());

    let interfaces = vec![
        ModelInterface::Sklearn(SklearnMeta {
            framework_version: "1.4.2".to_string(),
            model_subtype: Some("RandomForestClassifier".to_string()),
        }),
        ModelInterface::Xgboost(XgboostMeta {
            framework_version: "2.0.3".to_string(),
            model_subtype: Some("XGBClassifier".to_string()),
        }),
        ModelInterface::Lightgbm(LightgbmMeta {
            framework_version: "4.3.0".to_string(),
            model_subtype: None,
        }),
        ModelInterface::Catboost(CatboostMeta {
            framework_version: "1.2.5".to_string(),
            model_subtype: None,
        }),
        ModelInterface::Torch(TorchMeta {
            framework_version: "2.3.0".to_string(),
            model_subtype: Some("MyNet".to_string()),
            save_format: TorchSaveFormat::Safetensors,
        }),
        ModelInterface::Torch(TorchMeta {
            framework_version: "2.3.0".to_string(),
            model_subtype: None,
            save_format: TorchSaveFormat::Pickle,
        }),
        ModelInterface::Lightning(LightningMeta {
            framework_version: "2.2.4".to_string(),
            model_subtype: None,
        }),
        ModelInterface::Tensorflow(TensorflowMeta {
            framework_version: "2.16.1".to_string(),
            model_subtype: None,
            save_format: TfSaveFormat::Keras,
        }),
        ModelInterface::Tensorflow(TensorflowMeta {
            framework_version: "2.16.1".to_string(),
            model_subtype: None,
            save_format: TfSaveFormat::SavedModel,
        }),
        huggingface_interface(HuggingFaceTask::TextClassification),
        ModelInterface::Custom(CustomMeta {
            framework_version: "0.1.0".to_string(),
            model_subtype: None,
            loader_module: "mypkg.loaders".to_string(),
            loader_class: "GraphLoader".to_string(),
            extra,
        }),
    ];

    for interface in interfaces {
        round_trip_spec(interface);
    }
}

#[test]
fn every_huggingface_task_round_trips_in_huggingface_meta() {
    let tasks = [
        HuggingFaceTask::TextClassification,
        HuggingFaceTask::TokenClassification,
        HuggingFaceTask::QuestionAnswering,
        HuggingFaceTask::Summarization,
        HuggingFaceTask::Translation,
        HuggingFaceTask::TextGeneration,
        HuggingFaceTask::FillMask,
        HuggingFaceTask::ZeroShotClassification,
        HuggingFaceTask::ImageClassification,
        HuggingFaceTask::ObjectDetection,
        HuggingFaceTask::ImageSegmentation,
        HuggingFaceTask::ImageToText,
        HuggingFaceTask::ImageToImage,
        HuggingFaceTask::TextToImage,
        HuggingFaceTask::DepthEstimation,
        HuggingFaceTask::AudioClassification,
        HuggingFaceTask::AutomaticSpeechRecognition,
        HuggingFaceTask::AudioToAudio,
        HuggingFaceTask::TextToSpeech,
        HuggingFaceTask::TabularClassification,
        HuggingFaceTask::TabularRegression,
        HuggingFaceTask::FeatureExtraction,
        HuggingFaceTask::SentenceSimilarity,
        HuggingFaceTask::Conversational,
        HuggingFaceTask::DocumentQuestionAnswering,
        HuggingFaceTask::VisualQuestionAnswering,
        HuggingFaceTask::TableQuestionAnswering,
        HuggingFaceTask::Embedding,
        HuggingFaceTask::MultipleChoice,
        HuggingFaceTask::Other,
    ];

    for task in tasks {
        round_trip_spec(huggingface_interface(task));
    }
}

#[test]
fn every_sample_input_kind_round_trips_json_and_yaml() {
    let kinds = [
        SampleInputKind::Pandas,
        SampleInputKind::Polars,
        SampleInputKind::Arrow,
        SampleInputKind::Numpy,
        SampleInputKind::Torch,
        SampleInputKind::Tf,
        SampleInputKind::Dict,
        SampleInputKind::List,
        SampleInputKind::Tuple,
        SampleInputKind::Str,
        SampleInputKind::None,
    ];

    for kind in kinds {
        assert_json_yaml_roundtrip(&SampleInput::new(kind));
        let mut spec = spec(huggingface_interface(HuggingFaceTask::TextGeneration));
        spec.sample_input = Some(SampleInput::new(kind));
        assert_json_yaml_roundtrip(&spec);
    }
}

#[test]
fn every_task_type_round_trips_json_and_yaml() {
    let task_types = [
        TaskType::BinaryClassification,
        TaskType::MultiClassClassification,
        TaskType::Regression,
        TaskType::Clustering,
        TaskType::AnomalyDetection,
        TaskType::Forecasting,
        TaskType::Generation,
        TaskType::Other,
    ];

    for task_type in task_types {
        assert_json_yaml_roundtrip(&task_type);
        let mut spec = spec(huggingface_interface(HuggingFaceTask::TextGeneration));
        spec.task_type = task_type;
        assert_json_yaml_roundtrip(&spec);
    }
}

#[test]
fn save_format_enums_round_trip_json_and_yaml() {
    for save_format in [TorchSaveFormat::Safetensors, TorchSaveFormat::Pickle] {
        assert_json_yaml_roundtrip(&save_format);
    }
    for save_format in [TfSaveFormat::Keras, TfSaveFormat::SavedModel] {
        assert_json_yaml_roundtrip(&save_format);
    }
}
