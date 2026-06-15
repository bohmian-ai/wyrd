use std::collections::BTreeMap;

use wyrd_spec::card::field::FieldSpec;
use wyrd_spec::card::model::{
    CatboostMeta, CustomMeta, HuggingFaceTask, HuggingfaceMeta, LightgbmMeta, LightningMeta,
    ModelCardError, ModelInterface, ModelSignature, ModelSpec, SampleInput, SampleInputKind,
    SklearnMeta, TaskType, TensorflowMeta, TfSaveFormat, TorchMeta, TorchSaveFormat, XgboostMeta,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::{CardName, ColumnName, SpaceName};
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
        space: SpaceName::new("default").expect("static space is valid"),
        uid: None,
    }
}

fn signature() -> ModelSignature {
    ModelSignature::new(vec![field("x", "float64")], vec![field("y", "bool")])
}

fn sklearn_interface() -> ModelInterface {
    ModelInterface::Sklearn(SklearnMeta {
        framework_version: "1.4.2".to_string(),
        model_subtype: Some("RandomForestClassifier".to_string()),
    })
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

fn custom_interface() -> ModelInterface {
    ModelInterface::Custom(CustomMeta {
        framework_version: "0.1.0".to_string(),
        model_subtype: None,
        loader_module: "mypkg.loaders".to_string(),
        loader_class: "GraphLoader".to_string(),
        extra: BTreeMap::from([("graph_backend".to_string(), "dgl".to_string())]),
    })
}

#[test]
fn model_spec_new_runs_validation() {
    let err = ModelSpec::new(
        sklearn_interface(),
        TaskType::BinaryClassification,
        ModelSignature::new(Vec::new(), vec![field("y", "bool")]),
        None,
        Vec::new(),
    )
    .unwrap_err();
    assert_eq!(err, ModelCardError::EmptyInputs);

    let spec = ModelSpec::new(
        sklearn_interface(),
        TaskType::BinaryClassification,
        signature(),
        None,
        Vec::new(),
    )
    .unwrap();
    assert_eq!(spec.task_type, TaskType::BinaryClassification);
}

#[test]
fn model_spec_validate_is_idempotent() {
    let spec = ModelSpec::new(
        sklearn_interface(),
        TaskType::BinaryClassification,
        signature(),
        None,
        Vec::new(),
    )
    .unwrap();

    spec.validate().expect("first validation");
    spec.validate().expect("second validation");
}

#[test]
fn model_spec_interface_kind_matches_variant_tag() {
    let spec = ModelSpec::new(
        sklearn_interface(),
        TaskType::BinaryClassification,
        signature(),
        None,
        Vec::new(),
    )
    .unwrap();
    assert_eq!(spec.interface_kind(), "Sklearn");
}

#[test]
fn model_spec_is_generation_only_for_generation_task() {
    let mut spec = ModelSpec::new(
        sklearn_interface(),
        TaskType::BinaryClassification,
        signature(),
        None,
        Vec::new(),
    )
    .unwrap();
    assert!(!spec.is_generation());

    spec.task_type = TaskType::Generation;
    assert!(spec.is_generation());
}

#[test]
fn model_spec_card_refs_iterates_declared_refs() {
    let refs = vec![
        model_ref("model"),
        model_ref("tokenizer"),
        model_ref("sample"),
    ];
    let spec = ModelSpec::new(
        huggingface_interface(HuggingFaceTask::TextClassification),
        TaskType::MultiClassClassification,
        signature(),
        Some(SampleInput::new(SampleInputKind::Pandas)),
        refs.clone(),
    )
    .unwrap();

    assert_eq!(
        spec.card_refs().collect::<Vec<_>>(),
        refs.iter().collect::<Vec<_>>()
    );
}

#[test]
fn model_spec_card_refs_empty_for_local_spec_without_refs() {
    let spec = ModelSpec::new(
        sklearn_interface(),
        TaskType::Regression,
        signature(),
        None,
        Vec::new(),
    )
    .unwrap();
    assert_eq!(spec.card_refs().count(), 0);
}

#[test]
fn model_interface_kind_loader_media_and_extension_are_stable() {
    let cases = vec![
        (
            sklearn_interface(),
            "Sklearn",
            "sklearn",
            "application/x-joblib",
            "joblib",
        ),
        (
            ModelInterface::Xgboost(XgboostMeta {
                framework_version: "2.0.3".to_string(),
                model_subtype: None,
            }),
            "Xgboost",
            "xgboost",
            "application/x-joblib",
            "joblib",
        ),
        (
            ModelInterface::Lightgbm(LightgbmMeta {
                framework_version: "4.3.0".to_string(),
                model_subtype: None,
            }),
            "Lightgbm",
            "lightgbm",
            "application/x-joblib",
            "joblib",
        ),
        (
            ModelInterface::Catboost(CatboostMeta {
                framework_version: "1.2.5".to_string(),
                model_subtype: None,
            }),
            "Catboost",
            "catboost",
            "application/x-joblib",
            "joblib",
        ),
        (
            ModelInterface::Torch(TorchMeta {
                framework_version: "2.3.0".to_string(),
                model_subtype: None,
                save_format: TorchSaveFormat::Safetensors,
            }),
            "Torch",
            "torch",
            "application/vnd.safetensors",
            "safetensors",
        ),
        (
            ModelInterface::Torch(TorchMeta {
                framework_version: "2.3.0".to_string(),
                model_subtype: None,
                save_format: TorchSaveFormat::Pickle,
            }),
            "Torch",
            "torch",
            "application/vnd.safetensors",
            "pt",
        ),
        (
            ModelInterface::Lightning(LightningMeta {
                framework_version: "2.2.4".to_string(),
                model_subtype: None,
            }),
            "Lightning",
            "pytorch-lightning",
            "application/x-pytorch-ckpt",
            "ckpt",
        ),
        (
            ModelInterface::Tensorflow(TensorflowMeta {
                framework_version: "2.16.1".to_string(),
                model_subtype: None,
                save_format: TfSaveFormat::Keras,
            }),
            "Tensorflow",
            "tensorflow",
            "application/vnd.keras+zip",
            "keras",
        ),
        (
            ModelInterface::Tensorflow(TensorflowMeta {
                framework_version: "2.16.1".to_string(),
                model_subtype: None,
                save_format: TfSaveFormat::SavedModel,
            }),
            "Tensorflow",
            "tensorflow",
            "application/vnd.keras+zip",
            "savedmodel",
        ),
        (
            huggingface_interface(HuggingFaceTask::Other),
            "Huggingface",
            "huggingface-transformers",
            "application/vnd.huggingface+bundle",
            "huggingface",
        ),
        (
            custom_interface(),
            "Custom",
            "custom-python-loader",
            "application/octet-stream",
            "bin",
        ),
    ];

    for (interface, kind, loader_family, media_type, extension) in cases {
        assert_eq!(interface.kind(), kind);
        assert_eq!(interface.loader_family(), loader_family);
        assert_eq!(interface.default_media_type(), media_type);
        assert_eq!(interface.default_extension(), extension);
    }
}

#[test]
fn model_signature_new_preserves_fields_without_validation() {
    let signature = ModelSignature::new(Vec::new(), Vec::new());
    assert!(signature.inputs().is_empty());
    assert!(signature.outputs().is_empty());
    assert_eq!(signature.validate(), Err(ModelCardError::EmptyInputs));
}

#[test]
fn model_signature_from_fields_validates_and_preserves_order() {
    let err = ModelSignature::from_fields(
        vec![field("x", "float64"), field("x", "int64")],
        vec![field("y", "bool")],
    )
    .unwrap_err();
    assert_eq!(
        err,
        ModelCardError::DuplicateFieldName {
            side: "inputs",
            name: "x".to_string()
        }
    );

    let signature = ModelSignature::from_fields(
        vec![field("x", "float64"), field("z", "int64")],
        vec![field("y", "bool")],
    )
    .unwrap();
    assert_eq!(signature.inputs()[0].name.as_str(), "x");
    assert_eq!(signature.inputs()[1].name.as_str(), "z");
    assert_eq!(signature.outputs()[0].name.as_str(), "y");
    assert!(signature.validate().is_ok());
}

#[test]
fn sample_input_new_and_kind_are_stable() {
    let sample = SampleInput::new(SampleInputKind::Pandas);
    assert_eq!(sample.kind(), SampleInputKind::Pandas);

    let none = SampleInput::new(SampleInputKind::None);
    assert_eq!(none.kind(), SampleInputKind::None);
}
