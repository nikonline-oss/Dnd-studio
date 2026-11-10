//! Character sheet data schemas: the subset of JSON Schema that system plugins use
//! (docs/plugin-format.md). Shared by the plugin loader and the game core, which checks every sheet
//! itself (ADR-0023).
//!
//! Supported: `type` (object, integer, string, boolean, array), `properties`, `required`,
//! `minimum`, `maximum`, `maxLength`, `enum`, `items`, `maxItems`, `default`, `title`,
//! `visible_to_players`.

use serde_json::{Map, Value};

/// Maximum nesting of the data schema (objects and arrays).
pub const MAX_SCHEMA_DEPTH: usize = 16;
const COMMON_KEYWORDS: [&str; 4] = ["type", "title", "default", "visible_to_players"];

/// Checks a data schema that already passed the meta-schema. The root must be an object.
pub fn check_schema(schema: &Value) -> Result<(), String> {
    if node_type(schema) != Some("object") {
        return Err("the root of the sheet schema must have \"type\": \"object\"".to_owned());
    }
    check_node(schema, "", 1)
}

fn check_node(node: &Value, at: &str, depth: usize) -> Result<(), String> {
    let here = if at.is_empty() { "/" } else { at };
    if depth > MAX_SCHEMA_DEPTH {
        return Err(format!(
            "{here}: nested deeper than {MAX_SCHEMA_DEPTH} levels"
        ));
    }
    let Some(obj) = node.as_object() else {
        return Err(format!("{here}: a schema node must be an object"));
    };
    let ty = node_type(node).unwrap_or_default();
    let allowed: &[&str] = match ty {
        "object" => &["properties", "required"],
        "integer" => &["minimum", "maximum", "enum"],
        "string" => &["maxLength", "enum"],
        "boolean" => &[],
        "array" => &["items", "maxItems"],
        other => return Err(format!("{here}: unsupported type {other:?}")),
    };
    for key in obj.keys() {
        if !COMMON_KEYWORDS.contains(&key.as_str()) && !allowed.contains(&key.as_str()) {
            return Err(format!(
                "{here}: \"{key}\" is not supported for type \"{ty}\""
            ));
        }
    }
    if let Some(flag) = obj.get("visible_to_players")
        && !flag.is_boolean()
    {
        return Err(format!("{here}: \"visible_to_players\" must be a boolean"));
    }
    match ty {
        "object" => {
            let props = obj.get("properties").and_then(Value::as_object);
            for name in obj
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if !props.is_some_and(|p| p.contains_key(name)) {
                    return Err(format!("{here}: required \"{name}\" is not in properties"));
                }
            }
            for (name, child) in props.into_iter().flatten() {
                check_node(child, &format!("{at}/properties/{name}"), depth + 1)?;
            }
        }
        "integer" => {
            if let (Some(min), Some(max)) = (int(obj, "minimum"), int(obj, "maximum"))
                && min > max
            {
                return Err(format!(
                    "{here}: minimum {min} is greater than maximum {max}"
                ));
            }
        }
        "array" => {
            let items = obj
                .get("items")
                .ok_or_else(|| format!("{here}: an array needs \"items\""))?;
            check_node(items, &format!("{at}/items"), depth + 1)?;
        }
        _ => {}
    }
    if let Some(values) = obj.get("enum").and_then(Value::as_array) {
        let mut plain = obj.clone();
        plain.remove("enum");
        let plain = Value::Object(plain);
        for value in values {
            check_data(&plain, value).map_err(|e| format!("{here}: enum value {value}: {e}"))?;
        }
    }
    if let Some(default) = obj.get("default") {
        check_data(node, default).map_err(|e| format!("{here}: default: {e}"))?;
    }
    Ok(())
}

/// Checks `data` against a sheet schema of the supported subset. Objects do not allow properties
/// that the schema does not declare; strings are measured in Unicode scalar values.
pub fn check_data(schema: &Value, data: &Value) -> Result<(), String> {
    check_value(schema, data, "")
}

fn check_value(node: &Value, value: &Value, at: &str) -> Result<(), String> {
    let here = if at.is_empty() { "/" } else { at };
    let empty = Map::new();
    let obj = node.as_object().unwrap_or(&empty);
    let ty = node_type(node).unwrap_or_default();
    match (ty, value) {
        ("object", Value::Object(fields)) => {
            let props = obj.get("properties").and_then(Value::as_object);
            for name in obj
                .get("required")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if !fields.contains_key(name) {
                    return Err(format!("{here}: \"{name}\" is required"));
                }
            }
            for (name, field) in fields {
                let child = props
                    .and_then(|p| p.get(name))
                    .ok_or_else(|| format!("{here}: unknown field \"{name}\""))?;
                check_value(child, field, &format!("{at}/{name}"))?;
            }
        }
        ("integer", Value::Number(n)) => {
            let n = n
                .as_i64()
                .ok_or_else(|| format!("{here}: {n} is not an integer"))?;
            if let Some(min) = int(obj, "minimum")
                && n < min
            {
                return Err(format!("{here}: {n} is less than {min}"));
            }
            if let Some(max) = int(obj, "maximum")
                && n > max
            {
                return Err(format!("{here}: {n} is greater than {max}"));
            }
        }
        ("string", Value::String(s)) => {
            if let Some(max) = int(obj, "maxLength")
                && s.chars().count() as i64 > max
            {
                return Err(format!("{here}: longer than {max} characters"));
            }
        }
        ("boolean", Value::Bool(_)) => {}
        ("array", Value::Array(items)) => {
            if let Some(max) = int(obj, "maxItems")
                && items.len() as i64 > max
            {
                return Err(format!("{here}: more than {max} items"));
            }
            let item = obj
                .get("items")
                .ok_or_else(|| format!("{here}: the schema has no \"items\""))?;
            for (i, v) in items.iter().enumerate() {
                check_value(item, v, &format!("{at}/{i}"))?;
            }
        }
        _ => return Err(format!("{here}: expected {ty}")),
    }
    if let Some(values) = obj.get("enum").and_then(Value::as_array)
        && !values.contains(value)
    {
        return Err(format!("{here}: {value} is not one of the allowed values"));
    }
    Ok(())
}

pub(crate) fn node_type(node: &Value) -> Option<&str> {
    node.get("type").and_then(Value::as_str)
}

fn int(obj: &Map<String, Value>, key: &str) -> Option<i64> {
    obj.get(key).and_then(Value::as_i64)
}

/// The schema with only the public fields left (ADR-0049, ADR-0062).
///
/// Derived values must be computed from what a recipient may see: applying the *full* schema's
/// defaults to a public sheet invents numbers for hidden fields (a player would read the default
/// hit points as if they were real). Pruning the schema first keeps defaults and formulas on the
/// public side of the boundary.
#[must_use]
pub fn public_schema(schema: &Value) -> Value {
    if is_public(schema) {
        return schema.clone();
    }
    let mut out = schema.clone();
    let Some(object) = out.as_object_mut() else {
        return out;
    };
    let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) else {
        return out;
    };
    let kept: Map<String, Value> = properties
        .iter()
        .filter_map(|(name, node)| {
            if is_public(node) {
                Some((name.clone(), node.clone()))
            } else if node_type(node) == Some("object") {
                let pruned = public_schema(node);
                let has_fields = pruned
                    .get("properties")
                    .and_then(Value::as_object)
                    .is_some_and(|map| !map.is_empty());
                has_fields.then(|| (name.clone(), pruned))
            } else {
                None
            }
        })
        .collect();
    *properties = kept;
    // `required` names fields a public recipient will not have; a missing one is not an error
    // here, defaults already ran, and a stale required list would reject a valid public sheet.
    object.remove("required");
    out
}

/// The part of sheet `data` that recipients other than the owner and GM may read: properties
/// marked `"visible_to_players": true` and, inside an unmarked object, its marked properties.
/// A marked scalar or object is copied whole. A marked array keeps its items, and each item's
/// fields follow their own flags (an item object marked whole is copied as is); an unmarked
/// array is omitted because even its length is information. Without a matching property in the
/// schema nothing is public.
#[must_use]
pub fn public_data(schema: &Value, data: &Value) -> Value {
    if is_public(schema) {
        return public_value(schema, data);
    }
    let mut out = Map::new();
    if let Value::Object(fields) = data {
        copy_public(schema, fields, &mut out);
    }
    Value::Object(out)
}

fn is_public(node: &Value) -> bool {
    node.get("visible_to_players")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn copy_public(schema: &Value, fields: &Map<String, Value>, out: &mut Map<String, Value>) {
    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return;
    };
    for (name, value) in fields {
        let Some(node) = properties.get(name) else {
            continue;
        };
        if is_public(node) {
            out.insert(name.clone(), public_value(node, value));
        } else if node_type(node) == Some("object") {
            let mut nested = Map::new();
            if let Value::Object(child_fields) = value {
                copy_public(node, child_fields, &mut nested);
            }
            if !nested.is_empty() {
                out.insert(name.clone(), Value::Object(nested));
            }
        }
    }
}

fn public_value(node: &Value, value: &Value) -> Value {
    match (node_type(node), value) {
        (Some("array"), Value::Array(items)) => {
            let Some(item_schema) = node.get("items") else {
                return value.clone();
            };
            Value::Array(
                items
                    .iter()
                    .map(|item| public_item(item_schema, item))
                    .collect(),
            )
        }
        _ => value.clone(),
    }
}

fn public_item(schema: &Value, value: &Value) -> Value {
    if is_public(schema) {
        return value.clone();
    }
    if node_type(schema) == Some("object") {
        let mut out = Map::new();
        if let Value::Object(fields) = value {
            copy_public(schema, fields, &mut out);
        }
        return Value::Object(out);
    }
    value.clone()
}

/// Fills what `data` leaves out with the schema's defaults: a missing property gets its
/// `default`, or — for an object without one — an object filled the same way. Present values are
/// kept as they are (check them with [`check_data`] afterwards).
#[must_use]
pub fn apply_defaults(schema: &Value, data: &Value) -> Value {
    match (node_type(schema), data) {
        (Some("object"), Value::Object(fields)) => {
            let mut out = fields.clone();
            if let Some(props) = schema.get("properties").and_then(Value::as_object) {
                for (name, child) in props {
                    match out.get(name) {
                        Some(present) => {
                            let filled = apply_defaults(child, present);
                            out.insert(name.clone(), filled);
                        }
                        None => {
                            if let Some(default) = default_of(child) {
                                out.insert(name.clone(), default);
                            }
                        }
                    }
                }
            }
            Value::Object(out)
        }
        (Some("array"), Value::Array(items)) => match schema.get("items") {
            Some(item) => Value::Array(items.iter().map(|v| apply_defaults(item, v)).collect()),
            None => data.clone(),
        },
        _ => data.clone(),
    }
}

/// The value of a missing node: its `default`, or an object of its children's defaults.
fn default_of(node: &Value) -> Option<Value> {
    if let Some(default) = node.get("default") {
        return Some(apply_defaults(node, default));
    }
    (node_type(node) == Some("object")).then(|| apply_defaults(node, &Value::Object(Map::new())))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "visible_to_players": true },
                "notes": { "type": "string" },
                "abilities": {
                    "type": "object",
                    "properties": {
                        "str": { "type": "integer", "visible_to_players": true },
                        "secret": { "type": "integer" }
                    }
                },
                "loot": {
                    "type": "array",
                    "items": { "type": "integer" }
                }
            }
        })
    }

    #[test]
    fn public_data_keeps_only_declared_fields() {
        let data = json!({
            "name": "Mira",
            "notes": "private",
            "abilities": { "str": 16, "secret": 3 },
            "loot": [1, 2, 3]
        });
        assert_eq!(
            public_data(&schema(), &data),
            json!({ "name": "Mira", "abilities": { "str": 16 } })
        );
    }

    #[test]
    fn unmarked_array_is_not_partially_public() {
        let data = json!({ "loot": [1, 2, 3] });
        assert_eq!(public_data(&schema(), &data), json!({}));
    }

    #[test]
    fn a_marked_array_filters_its_item_fields() {
        let schema = json!({
            "type": "object",
            "properties": {
                "spells": {
                    "type": "array",
                    "visible_to_players": true,
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "visible_to_players": true },
                            "notes": { "type": "string" }
                        }
                    }
                }
            }
        });
        let data = json!({
            "spells": [
                { "name": "Fireball", "notes": "private" },
                { "name": "Shield", "notes": "private" }
            ]
        });
        assert_eq!(
            public_data(&schema, &data),
            json!({ "spells": [{ "name": "Fireball" }, { "name": "Shield" }] })
        );
    }

    #[test]
    fn marked_object_is_copied_whole_and_root_flag_opens_everything() {
        let mut schema = schema();
        schema["properties"]["abilities"]["visible_to_players"] = json!(true);
        let data = json!({ "abilities": { "str": 16, "secret": 3 } });
        assert_eq!(public_data(&schema, &data), data);
        assert_eq!(
            public_data(
                &json!({ "type": "object", "visible_to_players": true }),
                &data
            ),
            data
        );
    }

    #[test]
    fn missing_or_unknown_data_yields_an_empty_object() {
        assert_eq!(public_data(&schema(), &json!({})), json!({}));
        assert_eq!(public_data(&schema(), &json!([1])), json!({}));
        assert_eq!(
            public_data(&schema(), &json!({ "unknown": { "name": "x" } })),
            json!({})
        );
    }

    #[test]
    fn visible_to_players_must_be_a_boolean() {
        let invalid = json!({
            "type": "object",
            "properties": { "name": { "type": "string", "visible_to_players": "yes" } }
        });
        assert!(check_schema(&invalid).is_err());
    }
}
