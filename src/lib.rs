#![deny(unsafe_code)]

use photocraft_plugin_sdk::api::{
    ApiError, CommandInvocation, ErrorCode, EventRequest, PanelResponse, Result,
};
use photocraft_plugin_sdk::serde_json::{Map, Value, json};

pub mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

fn invalid(field: &str, message: &str) -> ApiError {
    ApiError::at(ErrorCode::InvalidJson, format!("values.{field}"), message)
}

fn choice<'a>(
    values: &'a Map<String, Value>,
    field: &str,
    default: &'a str,
    allowed: &[&str],
) -> Result<&'a str> {
    match values.get(field) {
        None => Ok(default),
        Some(Value::String(value)) if allowed.contains(&value.as_str()) => Ok(value),
        _ => Err(invalid(field, "Choose one of the available options.")),
    }
}

fn unit_number(values: &Map<String, Value>, field: &str, default: f64) -> Result<f64> {
    match values.get(field) {
        None => Ok(default),
        Some(value) => value
            .as_f64()
            .filter(|number| number.is_finite() && (0.0..=1.0).contains(number))
            .ok_or_else(|| invalid(field, "Enter a number between 0 and 1.")),
    }
}

fn worker_options(values: &Value) -> Result<Map<String, Value>> {
    let values = values
        .as_object()
        .ok_or_else(|| invalid("", "Panel settings must be an object."))?;
    let output = match choice(values, "output", "Depth blur", &["Depth blur", "Depth map"])? {
        "Depth map" => "map",
        _ => "blur",
    };
    let side = match choice(
        values,
        "side",
        "Background only",
        &["Background only", "Foreground and background"],
    )? {
        "Foreground and background" => "both",
        _ => "background",
    };
    let focus = unit_number(values, "focus", 0.65)?;
    let range = unit_number(values, "range", 0.1)?;
    let radius = match values.get("radius") {
        None => 16,
        Some(value) => value
            .as_u64()
            .filter(|radius| *radius <= 64)
            .ok_or_else(|| invalid("radius", "Enter a whole blur radius from 0 to 64 pixels."))?,
    };
    let invert = match values.get("invert") {
        None => false,
        Some(Value::Bool(value)) => *value,
        _ => return Err(invalid("invert", "Depth-map inversion must be on or off.")),
    };
    Ok(Map::from_iter([
        ("output".into(), json!(output)),
        ("focus".into(), json!(focus)),
        ("range".into(), json!(range)),
        ("radius".into(), json!(radius)),
        ("side".into(), json!(side)),
        ("invert".into(), json!(invert)),
    ]))
}

fn handle(request: &EventRequest) -> Result<PanelResponse> {
    let mut response = PanelResponse {
        state: request.state.clone(),
        message: "Depth Anything V2 Small · Relative depth: 0 is far, 1 is near. Preview the result before applying. Invert affects depth maps only.".into(),
        ..PanelResponse::default()
    };
    if request.event != bindings::EVENT_0 {
        return Ok(response);
    }
    if !request.document.is_object() {
        return Err(ApiError::new(
            ErrorCode::InvalidJson,
            "Open an image before creating a depth result.",
        ));
    }
    let options = worker_options(&request.values)?;
    let is_map = options.get("output").and_then(Value::as_str) == Some("map");
    response.commands.push(CommandInvocation {
        command: "plugin.processImage".into(),
        params: Map::from_iter([
            ("processor".into(), json!("native")),
            ("options".into(), Value::Object(options)),
            ("preview".into(), json!(true)),
        ]),
    });
    response.message = if is_map {
        "Estimating relative depth and creating a depth-map preview…"
    } else {
        "Estimating relative depth and creating a depth-blur preview…"
    }
    .into();
    Ok(response)
}

photocraft_plugin_sdk::export_plugin!(bindings::MANIFEST_JSON, handle);

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_plugin_sdk::{GuestBuffers, serde_json};

    fn request(event: &str, values: Value, document: Value) -> EventRequest {
        EventRequest::parse(
            &serde_json::to_vec(&json!({
                "apiVersion":1,
                "host":{"version":1,"capabilities":[]},
                "event":event,
                "values":values,
                "document":document,
                "state":{"retained":[true, "settings", 7]}
            }))
            .unwrap(),
        )
        .unwrap()
    }

    fn generate(values: Value) -> Result<PanelResponse> {
        handle(&request(
            "generate",
            values,
            json!({"width":100,"height":80}),
        ))
    }

    #[test]
    fn manifest_declares_only_the_current_contract_and_scoped_image_permission() {
        let authoring = photocraft_plugin_sdk::api::AuthoringManifest::parse(include_bytes!(
            "../authoring.json"
        ))
        .unwrap();
        assert_eq!(photocraft_guest_api_version(), 1);
        assert_eq!(authoring.package.package_version, 2);
        let descriptor: Value = serde_json::from_str(bindings::MANIFEST_JSON).unwrap();
        assert_eq!(descriptor["apiVersion"], 1);
        assert_eq!(descriptor["id"], "org.photocraft.depth-studio");
        assert_eq!(descriptor["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(
            descriptor["permissions"],
            json!({"documentRead":true,"commands":["plugin.processImage"]})
        );
        assert_eq!(
            descriptor["capabilities"]["required"],
            json!([{"id":"host.commands.invoke","minVersion":1}])
        );
        assert_eq!(descriptor["capabilities"]["optional"], json!([]));
        assert_eq!(descriptor["panel"]["preview"], true);
        assert_eq!(bindings::EVENT_0, "generate");
    }

    #[test]
    fn missing_fields_use_descriptor_defaults_and_request_one_owned_preview() {
        let request = request("generate", json!({}), json!({"width":100,"height":80}));
        let response = handle(&request).unwrap();
        assert_eq!(response.state, request.state);
        assert_eq!(response.commands.len(), 1);
        assert_eq!(response.commands[0].command, "plugin.processImage");
        assert_eq!(
            Value::Object(response.commands[0].params.clone()),
            json!({
                "processor":"native", "preview":true,
                "options":{"output":"blur","focus":0.65,"range":0.1,"radius":16,"side":"background","invert":false}
            })
        );
        assert!(response.requests.is_empty() && response.view.is_none());
        let descriptor: Value = serde_json::from_str(bindings::MANIFEST_JSON).unwrap();
        let defaults = descriptor["panel"]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| {
                (
                    field["id"].as_str().unwrap().into(),
                    field["default"].clone(),
                )
            })
            .collect::<Map<_, _>>();
        assert_eq!(
            worker_options(&Value::Object(defaults)).unwrap(),
            worker_options(&json!({})).unwrap()
        );
    }

    #[test]
    fn both_outputs_and_blur_sides_translate_to_the_worker_contract() {
        for (output, expected_output) in [("Depth blur", "blur"), ("Depth map", "map")] {
            for (side, expected_side) in [
                ("Background only", "background"),
                ("Foreground and background", "both"),
            ] {
                let response = generate(json!({
                    "output":output,"side":side,"focus":0.25,"range":0.3,"radius":32,"invert":true
                }))
                .unwrap();
                assert_eq!(response.commands.len(), 1);
                assert_eq!(
                    response.commands[0].params["options"],
                    json!({"output":expected_output,"side":expected_side,"focus":0.25,"range":0.3,"radius":32,"invert":true})
                );
            }
        }
    }

    #[test]
    fn controls_accept_exact_boundaries() {
        for (focus, range, radius) in [(0, 0, 0), (1, 1, 64)] {
            assert!(generate(json!({"focus":focus,"range":range,"radius":radius})).is_ok());
        }
    }

    #[test]
    fn malformed_and_out_of_range_settings_fail_before_processing() {
        for invalid_values in [Value::Null, json!([]), json!(false), json!("settings")] {
            assert!(generate(invalid_values).is_err());
        }
        for field in ["focus", "range"] {
            for value in [
                json!(-0.01),
                json!(1.01),
                json!("0.5"),
                Value::Null,
                json!(true),
            ] {
                assert!(generate(json!({field:value})).is_err(), "{field}");
            }
        }
        for value in [
            json!(-1),
            json!(65),
            json!(0.5),
            json!(16.0),
            json!("16"),
            Value::Null,
        ] {
            assert!(generate(json!({"radius":value})).is_err());
        }
        for field in ["output", "side"] {
            for value in [json!("unknown"), json!(1), Value::Null, json!([])] {
                assert!(generate(json!({field:value})).is_err(), "{field}");
            }
        }
        for value in [json!("true"), json!(1), Value::Null] {
            assert!(generate(json!({"invert":value})).is_err());
        }
    }

    #[test]
    fn document_is_required_only_for_the_explicit_generate_action() {
        for document in [Value::Null, json!([]), json!(false), json!("document")] {
            let error = handle(&request("generate", json!({}), document)).unwrap_err();
            assert!(error.message.contains("Open an image"));
        }
        for event in [
            "init",
            "change",
            "host_response",
            "document_changed",
            "unknown",
        ] {
            let request = request(event, json!({"output":"invalid"}), Value::Null);
            let response = handle(&request).unwrap();
            assert_eq!(response.state, request.state);
            assert!(response.commands.is_empty() && response.requests.is_empty());
        }
    }

    #[test]
    fn additive_event_metadata_is_accepted_and_incompatible_api_is_rejected() {
        let mut wire = serde_json::to_value(request("init", json!({}), Value::Null)).unwrap();
        wire["futureEnvelopeMetadata"] = json!({"accepted":true});
        let parsed = EventRequest::parse(&serde_json::to_vec(&wire).unwrap()).unwrap();
        assert_eq!(handle(&parsed).unwrap().state, parsed.state);
        for version in [0, 2, 3] {
            wire["apiVersion"] = json!(version);
            assert!(EventRequest::parse(&serde_json::to_vec(&wire).unwrap()).is_err());
        }
    }

    #[test]
    fn sdk_error_response_preserves_state_without_effects() {
        let request = request(
            "generate",
            json!({"radius":65}),
            json!({"width":100,"height":80}),
        );
        let bytes = serde_json::to_vec(&request).unwrap();
        let mut buffers = GuestBuffers::new();
        let allocation = buffers.allocate(bytes.len()).unwrap();
        allocation.copy_from_slice(&bytes);
        let pointer = allocation.as_ptr();
        let response =
            PanelResponse::parse(buffers.respond(pointer, bytes.len(), handle).unwrap()).unwrap();
        assert_eq!(response.state, request.state);
        assert!(response.message.contains("0 to 64"));
        assert!(response.commands.is_empty() && response.requests.is_empty());
    }
}
