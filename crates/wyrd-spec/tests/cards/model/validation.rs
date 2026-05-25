use std::collections::BTreeMap;

use serde_json::json;
use wyrd_spec::card::field::{Dim, FieldSpec, is_canonical_dtype};
use wyrd_spec::card::model::validate::{ModelCardError, validate_model_spec};
use wyrd_spec::card::model::{
    CatboostMeta, CustomMeta, HuggingFaceTask, HuggingfaceMeta, LightgbmMeta, LightningMeta,
    ModelInterface, ModelSignature, ModelSpec, SampleInput, SampleInputKind, SklearnMeta, TaskType,
    TensorflowMeta, TfSaveFormat, TorchMeta, TorchSaveFormat, XgboostMeta,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::ColumnName;

fn col(name: &str) -> ColumnName {
    ColumnName::new(name).unwrap()
}

fn field(name: &str, dtype: &str) -> FieldSpec {
    FieldSpec::new(col(name), dtype)
}

fn valid_signature() -> ModelSignature {
    ModelSignature::new(
        vec![field("input", "float32")],
        vec![field("output", "float32")],
    )
}

fn valid_interface() -> ModelInterface {
    ModelInterface::Sklearn(SklearnMeta {
        framework_version: "1.4.0".to_string(),
        model_subtype: None,
    })
}

fn valid_spec() -> ModelSpec {
    ModelSpec {
        interface: valid_interface(),
        task_type: TaskType::Regression,
        signature: valid_signature(),
        sample_input: None,
        artifact_refs: Vec::new(),
    }
}

fn empty_framework_interfaces() -> Vec<(&'static str, ModelInterface)> {
    vec![
        (
            "Sklearn",
            ModelInterface::Sklearn(SklearnMeta {
                framework_version: String::new(),
                model_subtype: None,
            }),
        ),
        (
            "Xgboost",
            ModelInterface::Xgboost(XgboostMeta {
                framework_version: String::new(),
                model_subtype: None,
            }),
        ),
        (
            "Lightgbm",
            ModelInterface::Lightgbm(LightgbmMeta {
                framework_version: String::new(),
                model_subtype: None,
            }),
        ),
        (
            "Catboost",
            ModelInterface::Catboost(CatboostMeta {
                framework_version: String::new(),
                model_subtype: None,
            }),
        ),
        (
            "Torch",
            ModelInterface::Torch(TorchMeta {
                framework_version: String::new(),
                model_subtype: None,
                save_format: TorchSaveFormat::Safetensors,
            }),
        ),
        (
            "Lightning",
            ModelInterface::Lightning(LightningMeta {
                framework_version: String::new(),
                model_subtype: None,
            }),
        ),
        (
            "Tensorflow",
            ModelInterface::Tensorflow(TensorflowMeta {
                framework_version: String::new(),
                model_subtype: None,
                save_format: TfSaveFormat::Keras,
            }),
        ),
        (
            "Huggingface",
            ModelInterface::Huggingface(HuggingfaceMeta {
                framework_version: String::new(),
                model_subtype: None,
                hf_task: HuggingFaceTask::TextGeneration,
                repo_id: None,
                revision: None,
            }),
        ),
        (
            "Custom",
            ModelInterface::Custom(CustomMeta {
                framework_version: String::new(),
                model_subtype: None,
                loader_module: "pkg.loader".to_string(),
                loader_class: "Loader".to_string(),
                extra: BTreeMap::new(),
            }),
        ),
    ]
}

#[test]
fn signature_inputs_empty_rejected() {
    let mut spec = valid_spec();
    spec.signature.inputs.clear();
    assert_eq!(validate_model_spec(&spec), Err(ModelCardError::EmptyInputs));
}

#[test]
fn signature_outputs_empty_rejected() {
    let mut spec = valid_spec();
    spec.signature.outputs.clear();
    assert_eq!(
        validate_model_spec(&spec),
        Err(ModelCardError::EmptyOutputs)
    );
}

#[test]
fn duplicate_input_field_rejected() {
    let mut spec = valid_spec();
    spec.signature.inputs.push(field("input", "float64"));
    assert_eq!(
        validate_model_spec(&spec),
        Err(ModelCardError::DuplicateFieldName {
            side: "inputs",
            name: "input".to_string()
        })
    );
}

#[test]
fn duplicate_output_field_rejected() {
    let mut spec = valid_spec();
    spec.signature.outputs.push(field("output", "float64"));
    assert_eq!(
        validate_model_spec(&spec),
        Err(ModelCardError::DuplicateFieldName {
            side: "outputs",
            name: "output".to_string()
        })
    );
}

#[test]
fn same_name_allowed_across_sides_accepted() {
    let spec = ModelSpec {
        signature: ModelSignature::from_fields(
            vec![field("x", "int64")],
            vec![field("x", "int64")],
        )
        .unwrap(),
        ..valid_spec()
    };
    assert!(spec.validate().is_ok());
}

#[test]
fn framework_version_empty_rejected_for_all_variants() {
    for (expected, interface) in empty_framework_interfaces() {
        let spec = ModelSpec {
            interface,
            ..valid_spec()
        };
        assert_eq!(
            validate_model_spec(&spec),
            Err(ModelCardError::EmptyFrameworkVersion { variant: expected })
        );
    }
}

#[test]
fn canonical_dtype_accepted() {
    for dtype in [
        "bool",
        "int8",
        "uint64",
        "float16",
        "large_utf8",
        "large_binary",
        "date64",
        "time32[ms]",
        "time64[ns]",
        "timestamp[us, tz=UTC]",
        "duration[ns]",
        "decimal128(10, 2)",
        "decimal256(76, 0)",
        "list<int64>",
        "large_list<utf8>",
        "fixed_size_list<float32, 4>",
        "struct<a:int64,b:list<float32>>",
        "dictionary<int32, utf8>",
    ] {
        assert!(is_canonical_dtype(dtype), "{dtype} should be accepted");
    }
    let spec = ModelSpec {
        signature: ModelSignature::new(vec![field("x", "list<int64>")], vec![field("y", "bool")]),
        ..valid_spec()
    };
    assert!(validate_model_spec(&spec).is_ok());
}

#[test]
fn unknown_dtype_rejected() {
    for dtype in [
        "",
        "string",
        "bfloat16",
        "timestamp[day]",
        "dictionary<float32, utf8>",
    ] {
        assert!(!is_canonical_dtype(dtype), "{dtype} should be rejected");
    }
    let spec = ModelSpec {
        signature: ModelSignature::new(vec![field("x", "bfloat16")], vec![field("y", "float32")]),
        ..valid_spec()
    };
    assert_eq!(
        validate_model_spec(&spec),
        Err(ModelCardError::DtypeNormalizeFailed {
            dtype: "bfloat16".to_string()
        })
    );
}

#[test]
fn shape_fixed_zero_rejected() {
    let mut input = field("x", "float32");
    input.shape = vec![Dim::Fixed(0)];
    let spec = ModelSpec {
        signature: ModelSignature::new(vec![input], vec![field("y", "float32")]),
        ..valid_spec()
    };
    assert_eq!(
        validate_model_spec(&spec),
        Err(ModelCardError::ShapeFixedNonPositive {
            field: "x".to_string(),
            value: 0
        })
    );
}

#[test]
fn shape_fixed_negative_rejected() {
    let mut input = field("x", "float32");
    input.shape = vec![Dim::Fixed(-1)];
    let spec = ModelSpec {
        signature: ModelSignature::new(vec![input], vec![field("y", "float32")]),
        ..valid_spec()
    };
    assert!(matches!(
        validate_model_spec(&spec),
        Err(ModelCardError::ShapeFixedNonPositive { value: -1, .. })
    ));
}

#[test]
fn shape_dynamic_and_scalar_accepted() {
    let mut dynamic = field("x", "float32");
    dynamic.shape = vec![Dim::Dynamic(Some("batch".to_string()))];
    let spec = ModelSpec {
        signature: ModelSignature::new(vec![dynamic], vec![field("y", "float32")]),
        ..valid_spec()
    };
    assert!(validate_model_spec(&spec).is_ok());

    let spec = valid_spec();
    assert!(validate_model_spec(&spec).is_ok());
}

#[test]
fn hf_revision_and_repo_id_validation() {
    for revision in ["abcdef0", &"a".repeat(40)] {
        let spec = ModelSpec {
            interface: ModelInterface::Huggingface(HuggingfaceMeta {
                framework_version: "4.41.0".to_string(),
                model_subtype: None,
                hf_task: HuggingFaceTask::TextGeneration,
                repo_id: Some("acme/model".to_string()),
                revision: Some(revision.to_string()),
            }),
            ..valid_spec()
        };
        assert!(validate_model_spec(&spec).is_ok());
    }

    for revision in ["abcdef", "ABCDEF0", &"a".repeat(41)] {
        let spec = ModelSpec {
            interface: ModelInterface::Huggingface(HuggingfaceMeta {
                framework_version: "4.41.0".to_string(),
                model_subtype: None,
                hf_task: HuggingFaceTask::TextGeneration,
                repo_id: Some("acme/model".to_string()),
                revision: Some(revision.to_string()),
            }),
            ..valid_spec()
        };
        assert_eq!(
            validate_model_spec(&spec),
            Err(ModelCardError::HuggingfaceRevisionInvalid {
                value: revision.to_string()
            })
        );
    }

    let spec = ModelSpec {
        interface: ModelInterface::Huggingface(HuggingfaceMeta {
            framework_version: "4.41.0".to_string(),
            model_subtype: None,
            hf_task: HuggingFaceTask::TextGeneration,
            repo_id: Some(" ".to_string()),
            revision: None,
        }),
        ..valid_spec()
    };
    assert_eq!(
        validate_model_spec(&spec),
        Err(ModelCardError::HuggingfaceRepoIdEmpty)
    );
}

#[test]
fn hf_task_other_accepted() {
    let spec = ModelSpec {
        interface: ModelInterface::Huggingface(HuggingfaceMeta {
            framework_version: "4.41.0".to_string(),
            model_subtype: None,
            hf_task: HuggingFaceTask::Other,
            repo_id: None,
            revision: None,
        }),
        ..valid_spec()
    };
    assert!(validate_model_spec(&spec).is_ok());
}

#[test]
fn custom_loader_module_and_class_validation() {
    let mut spec = ModelSpec {
        interface: ModelInterface::Custom(CustomMeta {
            framework_version: "1.0.0".to_string(),
            model_subtype: None,
            loader_module: "pkg.loader".to_string(),
            loader_class: "Loader".to_string(),
            extra: BTreeMap::new(),
        }),
        ..valid_spec()
    };
    assert!(validate_model_spec(&spec).is_ok());

    spec.interface = ModelInterface::Custom(CustomMeta {
        framework_version: "1.0.0".to_string(),
        model_subtype: None,
        loader_module: " ".to_string(),
        loader_class: "Loader".to_string(),
        extra: BTreeMap::new(),
    });
    assert_eq!(
        validate_model_spec(&spec),
        Err(ModelCardError::CustomLoaderInvalid {
            field: "loader_module"
        })
    );

    spec.interface = ModelInterface::Custom(CustomMeta {
        framework_version: "1.0.0".to_string(),
        model_subtype: None,
        loader_module: "pkg.loader".to_string(),
        loader_class: String::new(),
        extra: BTreeMap::new(),
    });
    assert_eq!(
        validate_model_spec(&spec),
        Err(ModelCardError::CustomLoaderInvalid {
            field: "loader_class"
        })
    );
}

#[test]
fn model_helpers_are_stable() {
    let interface = ModelInterface::Torch(TorchMeta {
        framework_version: "2.4.0".to_string(),
        model_subtype: Some("Module".to_string()),
        save_format: TorchSaveFormat::Pickle,
    });
    assert_eq!(interface.kind(), "Torch");
    assert_eq!(interface.loader_family(), "torch");
    assert_eq!(
        interface.default_media_type(),
        "application/vnd.safetensors"
    );
    assert_eq!(interface.default_extension(), "pt");

    let sample = SampleInput::new(SampleInputKind::Dict);
    assert_eq!(sample.kind(), SampleInputKind::Dict);

    let spec = ModelSpec::new(
        interface,
        TaskType::Generation,
        valid_signature(),
        Some(sample),
        Vec::new(),
    )
    .unwrap();
    assert_eq!(spec.interface_kind(), "Torch");
    assert!(spec.is_generation());
    assert_eq!(spec.signature.inputs().len(), 1);
    assert_eq!(spec.signature.outputs().len(), 1);
    assert_eq!(spec.artifact_refs().count(), 0);
}

#[test]
fn modelcard_error_maps_to_public_wyrd_error_codes_and_details() {
    let error: WyrdError = ModelCardError::EmptyInputs.into();
    assert_eq!(error.code(), "WYRD_MODEL_400_MISSING_SIGNATURE");
    assert_eq!(
        error.as_problem_json()["details"],
        json!({ "side": "inputs" })
    );

    let error: WyrdError = ModelCardError::EmptyOutputs.into();
    assert_eq!(error.code(), "WYRD_MODEL_400_MISSING_SIGNATURE");
    assert_eq!(
        error.as_problem_json()["details"],
        json!({ "side": "outputs" })
    );

    let error: WyrdError = ModelCardError::DuplicateFieldName {
        side: "outputs",
        name: "score".to_string(),
    }
    .into();
    assert_eq!(error.code(), "WYRD_MODEL_400_VALIDATION");
    assert_eq!(error.as_problem_json()["details"]["name"], "score");

    let error: WyrdError = ModelCardError::EmptyFrameworkVersion { variant: "Sklearn" }.into();
    assert_eq!(error.code(), "WYRD_MODEL_400_VALIDATION");
    assert_eq!(
        error.as_problem_json()["details"]["interface_kind"],
        "Sklearn"
    );

    let error: WyrdError = ModelCardError::DtypeNormalizeFailed {
        dtype: "object".to_string(),
    }
    .into();
    assert_eq!(error.code(), "WYRD_MODEL_400_DTYPE_NORMALIZE_FAILED");
    assert_eq!(error.as_problem_json()["details"]["dtype"], "object");

    let error: WyrdError = ModelCardError::ShapeFixedNonPositive {
        field: "x".to_string(),
        value: 0,
    }
    .into();
    assert_eq!(error.code(), "WYRD_MODEL_400_SHAPE_INVALID");
    assert_eq!(error.as_problem_json()["details"]["field"], "x");

    let error: WyrdError = ModelCardError::HuggingfaceRevisionInvalid {
        value: "ABCDEF0".to_string(),
    }
    .into();
    assert_eq!(error.code(), "WYRD_MODEL_400_HF_REVISION_INVALID");

    let error: WyrdError = ModelCardError::HuggingfaceRepoIdEmpty.into();
    assert_eq!(error.code(), "WYRD_MODEL_400_VALIDATION");
    assert_eq!(error.as_problem_json()["details"]["field"], "repo_id");

    let error: WyrdError = ModelCardError::HuggingfaceTaskMissing.into();
    assert_eq!(error.code(), "WYRD_MODEL_400_HF_TASK_MISSING");

    let error: WyrdError = ModelCardError::CustomLoaderInvalid {
        field: "loader_class",
    }
    .into();
    assert_eq!(error.code(), "WYRD_MODEL_400_CUSTOM_LOADER_INVALID");
    assert_eq!(error.as_problem_json()["details"]["field"], "loader_class");
}
