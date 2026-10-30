//! Plugins installed in the campaign and the rules of its system plugin (ADR-0005, ADR-0023).
//!
//! The client reads a `.dndplugin` and sends what it found; the core trusts none of it and checks
//! the rules again: the sheet schema stays inside the supported subset, every formula parses and
//! evaluates on a sheet of defaults (no cycles, no unknown fields, integer results), every dice
//! expression rolls. With a system plugin, character sheets are filled with the schema's defaults
//! and checked against it, and rolls see the computed fields (`1d20 + @str_mod`).

use std::collections::BTreeMap;

use dnd_dice::{FieldLookup, LookupError as DiceLookupError, SplitMix64};
use dnd_expr::{Computed, Context, Expr, LookupError, Value as ExprValue};
use dnd_protocol::{
    Event, Intent, InventoryDef, MAX_TRIGGER_TEMPLATES, PluginDto, PluginKind, RejectReason,
    SystemRulesDto, sheet_schema,
};
use serde_json::{Map, Value};

use crate::state::GameState;

/// Events for a plugin intent; `None` if `intent` is not about plugins.
pub(crate) fn apply(
    state: &GameState,
    intent: &Intent,
) -> Option<Result<Vec<Event>, RejectReason>> {
    let result = match intent {
        Intent::InstallPlugin { plugin } => (|| {
            check_plugin(plugin)?;
            // Replacing the active system with something that brings no rules would leave the
            // campaign without them.
            if state.system_plugin.as_deref() == Some(plugin.id.as_str()) && plugin.system.is_none()
            {
                return Err(RejectReason::invalid("plugin"));
            }
            Ok(vec![Event::PluginInstalled {
                plugin: plugin.clone(),
            }])
        })(),
        Intent::UninstallPlugin { id } => (|| {
            if !state.plugins.contains_key(id) {
                return Err(RejectReason::NotFound);
            }
            if state.system_plugin.as_deref() == Some(id.as_str()) {
                return Err(RejectReason::invalid("id"));
            }
            Ok(vec![Event::PluginUninstalled { id: id.clone() }])
        })(),
        Intent::SetSystemPlugin { id } => (|| {
            if let Some(id) = id {
                let plugin = state.plugins.get(id).ok_or(RejectReason::NotFound)?;
                if plugin.system.is_none() {
                    return Err(RejectReason::invalid("id"));
                }
            }
            if *id == state.system_plugin {
                return Err(RejectReason::invalid("id"));
            }
            Ok(vec![Event::SystemPluginSet { id: id.clone() }])
        })(),
        _ => return None,
    };
    Some(result)
}

/// Checks a plugin description as sent by a client.
pub fn check_plugin(plugin: &PluginDto) -> Result<(), RejectReason> {
    let sha = &plugin.archive_sha256;
    if sha.len() != 64 || !sha.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(RejectReason::invalid("archive_sha256"));
    }
    if plugin.version.trim().is_empty() || plugin.version.len() > 64 {
        return Err(RejectReason::invalid("version"));
    }
    // Trigger templates are data with blanks; the triggers made from them are checked as usual.
    let mut template_ids: Vec<&str> = plugin
        .trigger_templates
        .iter()
        .map(|t| t.id.as_str())
        .collect();
    template_ids.sort_unstable();
    let count = template_ids.len();
    template_ids.dedup();
    if count > MAX_TRIGGER_TEMPLATES || template_ids.len() != count {
        return Err(RejectReason::invalid("trigger_templates"));
    }
    match (&plugin.kind, &plugin.system) {
        (PluginKind::System, Some(rules)) => check_rules(rules),
        (PluginKind::System, None) => Err(RejectReason::invalid("system")),
        (_, Some(_)) => Err(RejectReason::invalid("system")),
        (_, None) => Ok(()),
    }
}

fn check_rules(rules: &SystemRulesDto) -> Result<(), RejectReason> {
    sheet_schema::check_schema(&rules.sheet_schema)
        .map_err(|_| RejectReason::invalid("sheet_schema"))?;
    if !rules.sheet_layout.is_object() {
        return Err(RejectReason::invalid("sheet_layout"));
    }
    if rules
        .creature_sheet_layout
        .as_ref()
        .is_some_and(|layout| !layout.is_object())
    {
        return Err(RejectReason::invalid("creature_sheet_layout"));
    }
    let thresholds = &rules.health_thresholds;
    if thresholds.len() != 3
        || thresholds.iter().any(|t| *t > 100)
        || thresholds.windows(2).any(|w| w[0] <= w[1])
    {
        return Err(RejectReason::invalid("health_thresholds"));
    }

    // Everything must work on a fresh character: a sheet of the schema's defaults.
    let sheet = default_sheet(rules);
    sheet_schema::check_data(&rules.sheet_schema, &sheet)
        .map_err(|_| RejectReason::invalid("sheet_schema"))?;
    let context = SheetContext(&sheet);
    let computed = Computed::resolve(&rules.computed, &context)
        .map_err(|_| RejectReason::invalid("computed"))?;
    let scope = computed.with_sheet(&context);

    for resource in &rules.resources {
        for expression in [&resource.value, &resource.max] {
            Expr::parse(expression)
                .and_then(|e| e.eval_integer(&scope))
                .map_err(|_| RejectReason::invalid("resources"))?;
        }
    }
    let fields = dnd_dice::ExprFields(scope);
    let dice = std::iter::once(rules.initiative.as_str())
        .chain(rules.rolls.iter().map(|r| r.dice.as_str()));
    for expression in dice {
        dnd_dice::Expr::parse(expression)
            .map_err(|_| RejectReason::invalid("dice"))?
            .roll(&fields, &mut SplitMix64::new(0))
            .map_err(|_| RejectReason::invalid("dice"))?;
    }
    let unique = |ids: Vec<&str>| {
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        sorted.len() == ids.len()
    };
    if !unique(rules.rolls.iter().map(|r| r.id.as_str()).collect())
        || !unique(rules.resources.iter().map(|r| r.id.as_str()).collect())
    {
        return Err(RejectReason::invalid("ids"));
    }
    let condition_id = |id: &str| {
        !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
    };
    if rules.conditions.len() > MAX_SYSTEM_CONDITIONS
        || !rules.conditions.iter().all(|c| condition_id(&c.id))
        || !unique(rules.conditions.iter().map(|c| c.id.as_str()).collect())
    {
        return Err(RejectReason::invalid("conditions"));
    }
    if let Some(inventory) = &rules.inventory
        && inventory_items(&rules.sheet_schema, inventory).is_none()
    {
        return Err(RejectReason::invalid("inventory"));
    }
    Ok(())
}

/// Most conditions a system may define.
pub const MAX_SYSTEM_CONDITIONS: usize = 64;

/// The schema of one inventory item: `path` leads (through object properties) to an array of
/// objects with a string property `name_field`.
fn inventory_items<'a>(schema: &'a Value, inventory: &InventoryDef) -> Option<&'a Value> {
    let mut node = schema;
    for segment in inventory.path.split('.') {
        node = node.get("properties")?.get(segment)?;
    }
    if node.get("type")?.as_str()? != "array" {
        return None;
    }
    let items = node.get("items")?;
    let name = items.get("properties")?.get(&inventory.name_field)?;
    (items.get("type")?.as_str()? == "object" && name.get("type")?.as_str()? == "string")
        .then_some(items)
}

/// The names of the items in a sheet's inventory list (empty without one).
#[must_use]
pub fn inventory_names(rules: &SystemRulesDto, sheet_json: &str) -> Vec<String> {
    let Some(inventory) = &rules.inventory else {
        return Vec::new();
    };
    let Ok(sheet) = serde_json::from_str::<Value>(sheet_json) else {
        return Vec::new();
    };
    let mut node = &sheet;
    for segment in inventory.path.split('.') {
        match node.get(segment) {
            Some(next) => node = next,
            None => return Vec::new(),
        }
    }
    node.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.get(&inventory.name_field)?.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The sheet with one more item named `name` in the system's inventory list (the item's other
/// fields take the schema's defaults), normalized like any sheet edit.
pub(crate) fn give_item(
    state: &GameState,
    sheet_json: &str,
    name: &str,
) -> Result<String, RejectReason> {
    let invalid = || RejectReason::invalid("name");
    let rules = state.rules().ok_or_else(invalid)?;
    let inventory = rules.inventory.as_ref().ok_or_else(invalid)?;
    let items = inventory_items(&rules.sheet_schema, inventory).ok_or_else(invalid)?;
    let mut fields = Map::new();
    fields.insert(inventory.name_field.clone(), Value::String(name.to_owned()));
    let item = sheet_schema::apply_defaults(items, &Value::Object(fields));
    let mut sheet: Value = serde_json::from_str(sheet_json).map_err(|_| invalid())?;
    let mut node = &mut sheet;
    for segment in inventory.path.split('.') {
        let object = node.as_object_mut().ok_or_else(invalid)?;
        node = object
            .entry(segment)
            .or_insert_with(|| Value::Object(Map::new()));
    }
    if !node.is_array() {
        *node = Value::Array(Vec::new());
    }
    node.as_array_mut().ok_or_else(invalid)?.push(item);
    let sheet = serde_json::to_string(&sheet).map_err(|_| invalid())?;
    normalize_sheet(state, &sheet)
}

/// A sheet of the schema's defaults (what a new character starts with).
#[must_use]
pub fn default_sheet(rules: &SystemRulesDto) -> Value {
    sheet_schema::apply_defaults(&rules.sheet_schema, &Value::Object(Map::new()))
}

/// The sheet a character intent stores: with a system, filled with defaults and checked against
/// its schema (returned normalized); without one, as sent (only the integer rule applies, checked
/// by the caller).
pub(crate) fn normalize_sheet(state: &GameState, sheet_json: &str) -> Result<String, RejectReason> {
    let Some(rules) = state.rules() else {
        return Ok(sheet_json.to_owned());
    };
    let invalid = || RejectReason::invalid("sheet_json");
    let data: Value = serde_json::from_str(sheet_json).map_err(|_| invalid())?;
    let filled = sheet_schema::apply_defaults(&rules.sheet_schema, &data);
    sheet_schema::check_data(&rules.sheet_schema, &filled).map_err(|_| invalid())?;
    serde_json::to_string(&filled).map_err(|_| invalid())
}

/// A character sheet as a `dnd-expr` context: integers and booleans at dotted paths.
#[derive(Clone, Copy, Debug)]
pub struct SheetContext<'a>(pub &'a Value);

impl Context for SheetContext<'_> {
    fn lookup(&self, path: &str) -> Result<ExprValue, LookupError> {
        let mut node = self.0;
        for segment in path.split('.') {
            node = node.get(segment).ok_or(LookupError::Unknown)?;
        }
        match node {
            Value::Number(n) => n
                .as_i64()
                .map(ExprValue::from)
                .ok_or_else(|| LookupError::Other("not an integer".to_owned())),
            Value::Bool(b) => Ok(ExprValue::from(*b)),
            _ => Err(LookupError::Other("not a number".to_owned())),
        }
    }
}

/// What `@field` means in a roll for a character: the computed fields of the campaign's system
/// (if any), then the integers of the sheet.
#[derive(Debug, Default)]
pub struct CharacterFields {
    sheet: Value,
    computed: BTreeMap<String, i64>,
}

impl CharacterFields {
    #[must_use]
    pub fn new(state: &GameState, sheet_json: Option<&str>) -> Self {
        let sheet = sheet_json
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(Value::Null);
        let computed = match state.rules() {
            Some(rules) => {
                let filled = sheet_schema::apply_defaults(&rules.sheet_schema, &sheet);
                Computed::resolve(&rules.computed, &SheetContext(&filled))
                    .map(Computed::into_values)
                    .unwrap_or_default()
            }
            None => BTreeMap::new(),
        };
        Self { sheet, computed }
    }

    /// The computed fields (for the sheet view).
    #[must_use]
    pub fn computed(&self) -> &BTreeMap<String, i64> {
        &self.computed
    }
}

impl FieldLookup for CharacterFields {
    fn lookup(&self, path: &str) -> Result<i64, DiceLookupError> {
        if let Some(value) = self.computed.get(path) {
            return Ok(*value);
        }
        let mut node = &self.sheet;
        for segment in path.split('.') {
            node = node.get(segment).ok_or(DiceLookupError::Unknown)?;
        }
        node.as_i64().ok_or(DiceLookupError::NotAnInteger)
    }
}

/// The same fields for `dnd-expr` (trigger conditions, ADR-0024): integers and booleans.
impl Context for CharacterFields {
    fn lookup(&self, path: &str) -> Result<ExprValue, LookupError> {
        if let Some(value) = self.computed.get(path) {
            return Ok(ExprValue::from(*value));
        }
        SheetContext(&self.sheet).lookup(path)
    }
}

/// What a sheet shows besides its data: the system's computed fields and resources, for a sheet
/// as it is being edited. Fields or resources that cannot be evaluated (a half-filled draft) are
/// left out rather than failing the whole view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SheetValues {
    pub computed: BTreeMap<String, i64>,
    /// `(id, value, max)` of each resource.
    pub resources: Vec<(String, i64, i64)>,
}

/// [`SheetValues`] of `sheet_json` under the campaign's system; `None` without a system.
#[must_use]
pub fn sheet_values(state: &GameState, sheet_json: &str) -> Option<SheetValues> {
    let rules = state.rules()?;
    let data: Value = serde_json::from_str(sheet_json).unwrap_or(Value::Object(Map::new()));
    let sheet = sheet_schema::apply_defaults(&rules.sheet_schema, &data);
    let context = SheetContext(&sheet);
    let computed = Computed::resolve(&rules.computed, &context).ok();
    let mut values = SheetValues::default();
    if let Some(computed) = &computed {
        values.computed = computed.values().clone();
        let scope = computed.with_sheet(&context);
        let eval = |source: &str| {
            Expr::parse(source)
                .and_then(|e| e.eval_integer(&scope))
                .ok()
        };
        for resource in &rules.resources {
            if let (Some(value), Some(max)) = (eval(&resource.value), eval(&resource.max)) {
                values.resources.push((resource.id.clone(), value, max));
            }
        }
    }
    Some(values)
}
