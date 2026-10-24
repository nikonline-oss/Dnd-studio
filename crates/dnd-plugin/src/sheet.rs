//! Character sheet: the data schema (a JSON Schema subset) and the layout that references it.
//!
//! The meta-schema (`schemas/sheet-schema.schema.json`) fixes which keywords exist and the shape
//! of their values; this module adds what a meta-schema cannot say: which keywords fit which
//! `type`, `required` names that exist, `minimum <= maximum`, defaults and enum values that match
//! their own node, nesting depth. [`check_data`] is the reference for checking sheet data against
//! such a schema; the game core implements the same rules itself (it cannot depend on this crate).

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use crate::model::{LayoutNode, SystemRules, Widget};

/// Maximum nesting of the data schema (objects and arrays).
pub const MAX_SCHEMA_DEPTH: usize = 16;
/// Maximum nesting of the layout tree.
pub const MAX_LAYOUT_DEPTH: usize = 32;

const COMMON_KEYWORDS: [&str; 3] = ["type", "title", "default"];

/// Checks a data schema that already passed the meta-schema. The root must be an object.
pub(crate) fn check_schema(schema: &Value) -> Result<(), String> {
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

fn node_type(node: &Value) -> Option<&str> {
    node.get("type").and_then(Value::as_str)
}

fn int(obj: &Map<String, Value>, key: &str) -> Option<i64> {
    obj.get(key).and_then(Value::as_i64)
}

/// Finds the schema node of a dotted path (`abilities.str`) below an object node.
fn resolve<'a>(scope: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(scope, |node, segment| node.get("properties")?.get(segment))
}

/// What the layout may reference.
struct Refs<'a> {
    computed: BTreeSet<&'a str>,
    rolls: BTreeSet<&'a str>,
    resources: BTreeSet<&'a str>,
}

/// Checks that every reference in the layout exists and every field widget fits its data type.
pub(crate) fn check_layout(
    root: &LayoutNode,
    schema: &Value,
    rules: &SystemRules,
) -> Result<(), String> {
    let refs = Refs {
        computed: rules.computed.keys().map(String::as_str).collect(),
        rolls: rules.rolls.iter().map(|r| r.id.as_str()).collect(),
        resources: rules.resources.iter().map(|r| r.id.as_str()).collect(),
    };
    check_layout_node(root, schema, &refs, 1)
}

fn check_layout_node(
    node: &LayoutNode,
    scope: &Value,
    refs: &Refs<'_>,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_LAYOUT_DEPTH {
        return Err(format!(
            "the layout is nested deeper than {MAX_LAYOUT_DEPTH} levels"
        ));
    }
    let children = |nodes: &[LayoutNode], scope: &Value| -> Result<(), String> {
        nodes
            .iter()
            .try_for_each(|n| check_layout_node(n, scope, refs, depth + 1))
    };
    match node {
        LayoutNode::Tabs { tabs } => tabs.iter().try_for_each(|t| children(&t.children, scope)),
        LayoutNode::Section { children: c, .. }
        | LayoutNode::Row { children: c }
        | LayoutNode::Grid { children: c, .. } => children(c, scope),
        LayoutNode::Field { path, widget, .. } => {
            let field = resolve(scope, path)
                .ok_or_else(|| format!("field \"{path}\" is not in the sheet schema"))?;
            check_widget(path, field, *widget)
        }
        LayoutNode::Computed { name, .. } => known(&refs.computed, name, "computed value"),
        LayoutNode::Roll { roll, .. } => known(&refs.rolls, roll, "roll"),
        LayoutNode::Resource { resource } => known(&refs.resources, resource, "resource"),
        LayoutNode::List { path, item, .. } => {
            let list = resolve(scope, path)
                .ok_or_else(|| format!("list \"{path}\" is not in the sheet schema"))?;
            let element = (node_type(list) == Some("array"))
                .then(|| list.get("items"))
                .flatten()
                .filter(|items| node_type(items) == Some("object"))
                .ok_or_else(|| format!("list \"{path}\" must be an array of objects"))?;
            children(item, element)
        }
        LayoutNode::Text { .. } => Ok(()),
    }
}

fn known(set: &BTreeSet<&str>, name: &str, what: &str) -> Result<(), String> {
    if set.contains(name) {
        Ok(())
    } else {
        Err(format!("{what} \"{name}\" is not defined in system.json"))
    }
}

fn check_widget(path: &str, field: &Value, widget: Option<Widget>) -> Result<(), String> {
    let ty = node_type(field).unwrap_or_default();
    let has_enum = field.get("enum").is_some();
    let fits = match (ty, widget) {
        ("integer", None | Some(Widget::Number)) => true,
        ("string", None | Some(Widget::Text | Widget::Textarea)) => true,
        ("boolean", None | Some(Widget::Checkbox)) => true,
        ("integer" | "string", Some(Widget::Select)) => has_enum,
        _ => false,
    };
    if fits {
        Ok(())
    } else if matches!(ty, "object" | "array") {
        Err(format!(
            "field \"{path}\" is an {ty}; only integer, string and boolean fields can be edited"
        ))
    } else {
        Err(format!(
            "widget {widget:?} does not fit field \"{path}\" of type {ty}{}",
            if widget == Some(Widget::Select) {
                " without enum"
            } else {
                ""
            }
        ))
    }
}
