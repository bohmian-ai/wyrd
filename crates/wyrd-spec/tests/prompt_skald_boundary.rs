mod prompt_support;

use prompt_support::{
    anthropic_with_system, openai_chat_request, prompt_card, prompt_spec, raw_request,
};
use skald_spec::{ProviderName, ProviderRequest};

#[test]
fn stored_anthropic_promptcard_round_trips_through_gateway_parse_path() {
    assert_stored_equals_proxied(
        anthropic_with_system("hi {{name}}"),
        vec![("name", "world")],
    );
}

#[test]
fn stored_openai_promptcard_round_trips_through_gateway_parse_path() {
    assert_stored_equals_proxied(openai_chat_request("hi {{name}}"), vec![("name", "world")]);
}

#[test]
fn stored_raw_v1_promptcard_round_trips_byte_identical() {
    let request = raw_request(ProviderName::Custom("acme".to_owned()));
    let spec = prompt_spec(request.clone(), Vec::new());
    let card = prompt_card(spec);
    let bytes = serde_json::to_vec(&card).expect("card serializes");
    let decoded: wyrd_spec::envelope::Card = serde_json::from_slice(&bytes).expect("card parses");
    let rendered = match decoded.spec {
        wyrd_spec::envelope::Spec::Prompt(spec) => spec.prompt.render(&[]).expect("renders"),
        _ => unreachable!("prompt card"),
    };
    let rendered_bytes = serde_json::to_vec(&rendered).expect("request serializes");
    let proxied: ProviderRequest = serde_json::from_slice(&rendered_bytes).expect("gateway parses");

    assert_eq!(rendered, proxied);
    assert_eq!(
        rendered_bytes,
        serde_json::to_vec(&proxied).expect("reserializes")
    );
    assert_eq!(request, proxied);
}

fn assert_stored_equals_proxied(request: ProviderRequest, vars: Vec<(&str, &str)>) {
    let variables = vars.iter().map(|(name, _)| *name).collect();
    let spec = prompt_spec(request, variables);
    let card = prompt_card(spec);
    let bytes = serde_json::to_vec(&card).expect("card serializes");
    let decoded: wyrd_spec::envelope::Card = serde_json::from_slice(&bytes).expect("card parses");
    let rendered = match decoded.spec {
        wyrd_spec::envelope::Spec::Prompt(spec) => spec.prompt.render(&vars).expect("renders"),
        _ => unreachable!("prompt card"),
    };
    let rendered_bytes = serde_json::to_vec(&rendered).expect("request serializes");
    let proxied: ProviderRequest = serde_json::from_slice(&rendered_bytes).expect("gateway parses");

    assert_eq!(rendered, proxied);
    assert_eq!(
        rendered_bytes,
        serde_json::to_vec(&proxied).expect("reserializes")
    );
}
