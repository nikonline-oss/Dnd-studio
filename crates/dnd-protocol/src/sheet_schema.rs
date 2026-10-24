//! Character sheet data schemas: the subset of JSON Schema that system plugins use
//! (docs/plugin-format.md). Shared by the plugin loader and the game core, which checks every sheet
//! itself (ADR-0023).
//!
//! Supported: `type` (object, integer, string, boolean, array), `properties`, `required`,
//! `minimum`, `maximum`, `maxLength`, `enum`, `items`, `maxItems`, `default`, `title`.

use serde_json::{Map, Value};

/// Maximum nesting of the data schema (objects and arrays).
pub const MAX_SCHEMA_DEPTH: usize = 16;
const COMMON_KEYWORDS: [&str; 3] = ["type", "title", "default"];

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
