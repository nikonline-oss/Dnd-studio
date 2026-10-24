//! Character sheet: the data schema (a JSON Schema subset) and the layout that references it.
//!
//! The meta-schema (`schemas/sheet-schema.schema.json`) fixes which keywords exist and the shape
//! of their values; this module adds what a meta-schema cannot say: which keywords fit which
//! `type`, `required` names that exist, `minimum <= maximum`, defaults and enum values that match
//! their own node, nesting depth. [`check_data`] is the reference for checking sheet data against
//! such a schema; the game core implements the same rules itself (it cannot depend on this crate).

use std::collections::BTreeSet;

pub use dnd_protocol::sheet_schema::{MAX_SCHEMA_DEPTH, apply_defaults, check_data, check_schema};
use serde_json::Value;

use crate::model::{LayoutNode, SystemRules, Widget};

/// Maximum nesting of the layout tree.
pub const MAX_LAYOUT_DEPTH: usize = 32;

fn node_type(node: &Value) -> Option<&str> {
    node.get("type").and_then(Value::as_str)
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
