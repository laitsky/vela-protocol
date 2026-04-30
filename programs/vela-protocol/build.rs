use serde_json::Value;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
    let build_dir = manifest_dir.join("../../build");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("missing OUT_DIR"));

    println!("cargo:rerun-if-changed={}", build_dir.display());

    let validate_params = load_inputs(&build_dir, "validate_mandate");
    let validate_outputs = load_outputs(&build_dir, "validate_mandate");
    let validate_weight = load_weight(&build_dir, "validate_mandate");

    let billing_params = load_inputs(&build_dir, "record_billing_event");
    let billing_outputs = load_outputs(&build_dir, "record_billing_event");
    let billing_weight = load_weight(&build_dir, "record_billing_event");

    let generated = format!(
        "use arcium_client::idl::arcium::types::{{Output, Parameter}};\n\
         pub const VALIDATE_MANDATE_PARAMS: [Parameter; {validate_params_len}] = {validate_params};\n\
         pub const VALIDATE_MANDATE_OUTPUTS: [Output; {validate_outputs_len}] = {validate_outputs};\n\
         pub const VALIDATE_MANDATE_WEIGHT: u64 = {validate_weight};\n\
         pub const RECORD_BILLING_EVENT_PARAMS: [Parameter; {billing_params_len}] = {billing_params};\n\
         pub const RECORD_BILLING_EVENT_OUTPUTS: [Output; {billing_outputs_len}] = {billing_outputs};\n\
         pub const RECORD_BILLING_EVENT_WEIGHT: u64 = {billing_weight};\n",
        validate_params_len = validate_params.len(),
        validate_params = rust_array(&validate_params),
        validate_outputs_len = validate_outputs.len(),
        validate_outputs = rust_array(&validate_outputs),
        validate_weight = validate_weight,
        billing_params_len = billing_params.len(),
        billing_params = rust_array(&billing_params),
        billing_outputs_len = billing_outputs.len(),
        billing_outputs = rust_array(&billing_outputs),
        billing_weight = billing_weight,
    );

    fs::write(out_dir.join("circuit_metadata.rs"), generated)
        .expect("failed to write generated circuit metadata");
}

fn load_inputs(build_dir: &Path, circuit: &str) -> Vec<String> {
    let json = load_idarc(build_dir, circuit);
    flatten_schema_array(
        json.get("inputs").and_then(Value::as_array),
        SchemaKind::Input,
    )
}

fn load_outputs(build_dir: &Path, circuit: &str) -> Vec<String> {
    let json = load_idarc(build_dir, circuit);
    flatten_schema_array(
        json.get("outputs").and_then(Value::as_array),
        SchemaKind::Output,
    )
}

fn load_idarc(build_dir: &Path, circuit: &str) -> Value {
    let path = build_dir.join(format!("{circuit}.idarc"));
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|err| panic!("failed to parse {}: {err}", path.display()))
}

fn load_weight(build_dir: &Path, circuit: &str) -> u64 {
    let path = build_dir.join(format!("{circuit}.weight"));
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let json: Value = serde_json::from_str(&raw)
        .unwrap_or_else(|err| panic!("failed to parse {}: {err}", path.display()));
    json.get("weight")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("missing weight field in {}", path.display()))
}

fn flatten_schema_array(nodes: Option<&Vec<Value>>, kind: SchemaKind) -> Vec<String> {
    let mut flattened = Vec::new();
    for node in nodes.into_iter().flatten() {
        flatten_node(node, kind, &mut flattened);
    }
    flattened
}

fn flatten_node(node: &Value, kind: SchemaKind, flattened: &mut Vec<String>) {
    let Some(node_type) = node.get("type").and_then(Value::as_str) else {
        return;
    };

    match node_type {
        "struct" | "array" => {
            if let Some(children) = node.get("content").and_then(Value::as_array) {
                for child in children {
                    flatten_node(child, kind, flattened);
                }
            }
        }
        leaf_type => flattened.push(map_leaf_type(leaf_type, kind)),
    }
}

fn map_leaf_type(leaf_type: &str, kind: SchemaKind) -> String {
    match (kind, leaf_type) {
        (SchemaKind::Input, "arcis_x25519_pubkey") => "Parameter::ArcisX25519Pubkey".into(),
        (SchemaKind::Input, "u128") => "Parameter::PlaintextU128".into(),
        (SchemaKind::Input, "u64") => "Parameter::PlaintextU64".into(),
        (SchemaKind::Input, "i64") => "Parameter::PlaintextI64".into(),
        (SchemaKind::Input, "bool") => "Parameter::PlaintextBool".into(),
        (SchemaKind::Input, "ciphertext") => "Parameter::Ciphertext".into(),
        (SchemaKind::Output, "u128") => "Output::PlaintextU128".into(),
        (SchemaKind::Output, "u64") => "Output::PlaintextU64".into(),
        (SchemaKind::Output, "i64") => "Output::PlaintextI64".into(),
        (SchemaKind::Output, "bool") => "Output::PlaintextBool".into(),
        (SchemaKind::Output, "ciphertext") => "Output::Ciphertext".into(),
        _ => panic!("unsupported circuit schema leaf: {leaf_type} ({kind:?})"),
    }
}

fn rust_array(items: &[String]) -> String {
    format!("[{}]", items.join(", "))
}

#[derive(Clone, Copy, Debug)]
enum SchemaKind {
    Input,
    Output,
}
