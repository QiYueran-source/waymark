//! Cross-phase tool-surface invariants.
//!
//! These helpers are deliberately public and phase-independent: phase A uses
//! them to prove the read/query surface accepts no batch form, and phase B must
//! apply them to every write tool it adds. The rule comes from the shared
//! business contract: there is no `items` array, no multiple-ID form, and no
//! mixed-operation array on any write.
//!
//! The detection is schema-based so it works for the schema the tool macro
//! generates, not for a hand-maintained list of names.

use serde_json::Value;

/// Parameter names that would denote a batch target list on any tool.
const BATCH_PARAMETER_NAMES: &[&str] = &["items", "ids", "targets", "batch", "changes"];

/// The parameters of `input_schema` that accept an array of targets.
///
/// A parameter is a batch when its name is one of [`BATCH_PARAMETER_NAMES`] or
/// when it declares an array of objects (one entry per target). Scalar lists
/// such as `fields` (an array of strings) are projections, not targets, and are
/// not reported.
pub fn batch_parameters(input_schema: &Value) -> Vec<String> {
    let Some(properties) = input_schema.get("properties").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (name, schema) in properties {
        if BATCH_PARAMETER_NAMES.contains(&name.as_str()) || is_array_of_objects(schema) {
            found.push(name.clone());
        }
    }
    found.sort();
    found
}

/// Whether a parameter schema is an array whose elements are objects.
fn is_array_of_objects(schema: &Value) -> bool {
    if schema.get("type").and_then(Value::as_str) != Some("array") {
        return false;
    }
    matches!(
        schema.get("items"),
        Some(Value::Object(items)) if items.get("type").and_then(Value::as_str) == Some("object")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_helper_detects_an_items_array_and_a_multi_id_array() {
        let schema = json!({
            "type": "object",
            "properties": {
                "items": {"type": "array", "items": {"type": "object"}},
                "ids": {"type": "array", "items": {"type": "string"}},
                "targets": {"type": "array", "items": {"type": "object"}},
                "title": {"type": "string"}
            }
        });
        assert_eq!(
            batch_parameters(&schema),
            vec!["ids".to_owned(), "items".to_owned(), "targets".to_owned()]
        );
    }

    #[test]
    fn a_projection_field_list_is_not_a_batch_parameter() {
        let schema = json!({
            "type": "object",
            "properties": {
                "fields": {"type": "array", "items": {"type": "string"}},
                "id": {"type": "string"}
            }
        });
        assert!(batch_parameters(&schema).is_empty());
    }
}
