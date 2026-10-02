use serde_json::{Value, json};
use silicon_honeycomb_core::AppInput;

fn configuration(downstream: Value) -> Value {
    json!({
        "org_id":"work", "app_id":"waveform", "name":"Waveform",
        "description":"Description ".repeat(50), "base_url":"https://waveform.example",
        "webhook_url":"https://waveform.example/webhook", "webhook_secret":"s".repeat(32),
        "webhook_scope":["membership"],
        "obo_endpoints":[{
            "endpoint_id":"speech.generate", "path":"/speech", "metadata":{}, "critical":false,
            "downstream":downstream
        }]
    })
}

#[test]
fn downstream_calls_survive_public_config_and_edit_round_trip() {
    let calls = json!([
        {"audience":"briefcase","endpoint_id":"files.upload"},
        {"audience":"briefcase","endpoint_id":"files.read"}
    ]);
    let input: AppInput = serde_json::from_value(configuration(calls.clone())).unwrap();
    assert!(input.validate().is_empty(), "{:?}", input.validate());
    let mut public = input.public_config();
    assert!(public.get("webhook_secret").is_none());
    assert_eq!(public["obo_endpoints"][0]["downstream"], calls);
    public["name"] = json!("Updated name");
    public["webhook_secret"] = json!("s".repeat(32));
    let edited: AppInput = serde_json::from_value(public).unwrap();
    assert!(edited.validate().is_empty());
    assert_eq!(
        edited.public_config()["obo_endpoints"][0]["downstream"],
        calls
    );
}

#[test]
fn absent_dependencies_keep_legacy_configuration_shape() {
    let mut value = configuration(json!([]));
    value["obo_endpoints"][0]
        .as_object_mut()
        .unwrap()
        .remove("downstream");
    let input: AppInput = serde_json::from_value(value).unwrap();
    assert!(input.obo_endpoints[0].downstream.is_empty());
    assert!(
        input.public_config()["obo_endpoints"][0]
            .get("downstream")
            .is_none()
    );
}

#[test]
fn rejects_invalid_duplicate_self_and_excessive_dependencies() {
    for calls in [
        json!([{"audience":"provider","endpoint_id":"FILES.read"}]),
        json!([{"audience":"org>provider","endpoint_id":"files.read"}]),
        json!([{"audience":"provider","endpoint_id":""}]),
        json!([{"audience":"provider","endpoint_id":"x".repeat(129)}]),
        json!([{"audience":"waveform","endpoint_id":"another.endpoint"}]),
        json!([{"audience":"provider","endpoint_id":"files.read"},{"audience":"provider","endpoint_id":"files.read"}]),
        Value::Array(
            (0..17)
                .map(|i| json!({"audience":"provider","endpoint_id":format!("files.{i}")}))
                .collect(),
        ),
    ] {
        let input: AppInput = serde_json::from_value(configuration(calls.clone())).unwrap();
        assert!(
            input.validate().iter().any(|e| e.contains("downstream")),
            "accepted {calls}"
        );
    }
    let limit = Value::Array(
        (0..16)
            .map(|i| json!({"audience":"provider","endpoint_id":format!("files.{i}")}))
            .collect(),
    );
    let input: AppInput = serde_json::from_value(configuration(limit)).unwrap();
    assert!(input.validate().is_empty());
}

#[test]
fn malformed_dependencies_are_not_silently_discarded() {
    for calls in [
        json!([{"audience":"provider","endpoint_id":"files.read","unexpected":true}]),
        json!([{"audience":"provider"}]),
        json!(null),
    ] {
        assert!(serde_json::from_value::<AppInput>(configuration(calls)).is_err());
    }
}
