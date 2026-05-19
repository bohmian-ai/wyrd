use proptest::prelude::*;
use serde_json::json;
use wyrd_spec::card::service::{
    ServiceRuntime, ServiceRuntimeKind, ServiceRuntimeMode, ServiceRuntimePolicy, ServiceSpec,
};
use wyrd_spec::envelope::{Card, Spec};
use wyrd_spec::format;

#[test]
fn legacy_service_spec_without_runtime_round_trips_without_runtime_key() {
    let fixture = include_str!("fixtures/legacy-service-spec-without-runtime.json");
    let decoded: ServiceSpec = serde_json::from_str(fixture).unwrap();
    assert_eq!(decoded.runtime, None);

    let encoded = serde_json::to_string(&decoded).unwrap();
    assert_eq!(encoded, fixture.trim());
}

#[test]
fn service_runtime_fixture_round_trips() {
    let decoded: Card =
        format::yaml::from_str(include_str!("fixtures/service-with-runtime-policy.yaml")).unwrap();
    let Spec::Service(spec) = &decoded.spec else {
        panic!("expected Service spec");
    };
    let runtime = spec.runtime.as_ref().expect("runtime");
    assert_eq!(runtime.kind, ServiceRuntimeKind::Agent);
    assert_eq!(runtime.mode, Some(ServiceRuntimeMode::InProcess));
    assert_eq!(runtime.strict, Some(true));
    assert!(runtime.policy.as_ref().unwrap().runtime_hooks);

    let encoded = format::yaml::to_string(&decoded).unwrap();
    let reparsed: Card = format::yaml::from_str(&encoded).unwrap();
    assert_eq!(reparsed, decoded);
}

fn runtime_strategy() -> impl Strategy<Value = ServiceRuntime> {
    let kind = prop::sample::select(vec![
        ServiceRuntimeKind::Api,
        ServiceRuntimeKind::Mcp,
        ServiceRuntimeKind::Agent,
        ServiceRuntimeKind::Workflow,
    ]);
    let framework = prop::option::of("[a-z][a-z0-9_-]{0,12}".prop_map(String::from));
    let mode = prop::option::of(Just(ServiceRuntimeMode::InProcess));
    let strict = prop::option::of(any::<bool>());
    let policy = prop::option::of(
        any::<bool>().prop_map(|runtime_hooks| ServiceRuntimePolicy { runtime_hooks }),
    );
    let config = prop_oneof![
        Just(serde_json::Value::Null),
        Just(json!({"max_concurrent_invocations": 4})),
        Just(json!({"queue": "default"})),
    ];

    (kind, framework, mode, strict, policy, config).prop_map(
        |(kind, framework, mode, strict, policy, config)| ServiceRuntime {
            kind,
            framework,
            mode,
            strict,
            policy,
            config,
        },
    )
}

proptest! {
    #[test]
    fn service_runtime_round_trips(runtime in runtime_strategy()) {
        let encoded = serde_json::to_string(&runtime).unwrap();
        let decoded: ServiceRuntime = serde_json::from_str(&encoded).unwrap();
        prop_assert_eq!(decoded, runtime);
    }
}
