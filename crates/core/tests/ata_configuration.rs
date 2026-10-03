use serde_json::{Value, json};
use silicon_honeycomb_core::AppInput;
fn config() -> Value {
    json!({"org_id":"work","app_id":"waveform","name":"Waveform",
 "description":"Application speech service. ".repeat(50),"base_url":"https://waveform.example",
 "webhook_url":"https://waveform.example/webhook","webhook_secret":"s".repeat(32),"webhook_scope":["membership"],
 "ata_endpoints":[{"endpoint_id":"speech.generate","name":"Generate speech","path":"/speech",
 "description":"Generate application-owned audio","critical":true,"metadata":{"format":"mp3"},
 "note_to_user":"Charged to the application","additional_warnings":["uses_credits"],
 "downstream":[{"audience":"briefcase","endpoint_id":"files.upload"}]}]})
}
#[test]
fn ata_metadata_and_dependencies_round_trip_without_user_token_options() {
    let input: AppInput = serde_json::from_value(config()).unwrap();
    assert!(input.validate().is_empty(), "{:?}", input.validate());
    let result = input.public_config();
    assert_eq!(
        result["ata_endpoints"][0]["downstream"],
        config()["ata_endpoints"][0]["downstream"]
    );
    assert_eq!(
        result["ata_endpoints"][0]["additional_warnings"],
        json!(["uses_credits"])
    );
    assert!(result["ata_endpoints"][0].get("ttl_seconds").is_none());
    let mut invalid = config();
    invalid["ata_endpoints"][0]["ttl_seconds"] = json!(300);
    assert!(serde_json::from_value::<AppInput>(invalid).is_err());
}
#[test]
fn ata_rejects_obo_ids_unsafe_paths_and_unrecognized_warnings() {
    for (field, value) in [
        ("endpoint_id", json!("[waveform:obo:speech]")),
        ("name", json!("")),
        ("description", json!("")),
        ("path", json!("/files/../delete")),
        ("path", json!("/files%2fdelete")),
        ("metadata", json!([])),
        ("additional_warnings", json!(["unknown"])),
        (
            "additional_warnings",
            json!(["uses_credits", "uses_credits"]),
        ),
    ] {
        let mut invalid = config();
        invalid["ata_endpoints"][0][field] = value;
        let input: AppInput = serde_json::from_value(invalid).unwrap();
        assert!(!input.validate().is_empty(), "accepted invalid {field}");
    }
}
