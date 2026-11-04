//! Sheet rows supply only the parameters explicitly allowed by a trusted plugin.
use crate::GameState;
use dnd_protocol::*;
use serde_json::Value;
use std::collections::BTreeSet;

fn at<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(value, |v, part| v.get(part))
}
fn schema_at<'a>(schema: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(schema, |v, part| v.get("properties")?.get(part))
}
fn ident(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .enumerate()
            .all(|(i, c)| c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
}
fn path(s: &str) -> bool {
    s.len() <= 256 && s.split('.').all(ident)
}

pub(crate) fn validate(rules: &SystemRulesDto) -> Result<(), RejectReason> {
    let invalid = || RejectReason::invalid("sheet_action");
    let mut ids = BTreeSet::new();
    if rules.mechanics.sheet_actions.len() > 16 {
        return Err(invalid());
    }
    for list in &rules.mechanics.sheet_actions {
        let schema = schema_at(&rules.sheet_schema, &list.path).ok_or_else(invalid)?;
        let row = schema.get("items").ok_or_else(invalid)?;
        let is_field = |p: &str, ty: &str| {
            path(p)
                && schema_at(row, p)
                    .and_then(|s| s.get("type"))
                    .and_then(Value::as_str)
                    == Some(ty)
        };
        if !ident(&list.id)
            || !ids.insert(&list.id)
            || !path(&list.path)
            || schema.get("type").and_then(Value::as_str) != Some("array")
            || row.get("type").and_then(Value::as_str) != Some("object")
            || schema
                .get("maxItems")
                .and_then(Value::as_u64)
                .is_none_or(|n| n > 100)
            || !is_field(&list.name_field, "string")
            || list
                .action_field
                .as_ref()
                .is_some_and(|p| !is_field(p, "string"))
            || list.attack_dice.as_ref().is_some_and(|text| {
                dnd_dice::Expr::parse(text).map_or(true, |dice| {
                    dice.terms().iter().any(|t| {
                        if let dnd_dice::Term::Field(field) = &t.term
                            && let Some(p) = field.strip_prefix("row.")
                        {
                            return !is_field(p, "integer");
                        }
                        false
                    })
                })
            })
            || list.actions.is_empty()
            || list.actions.len() > 32
            || list.actions.iter().collect::<BTreeSet<_>>().len() != list.actions.len()
            || list.actions.iter().any(|id| {
                !rules
                    .mechanics
                    .actions
                    .iter()
                    .any(|a| &a.id == id && !a.reaction_only && a.interrupt.is_none())
            })
            || list.bindings.iter().any(|(field, p)| {
                !is_field(
                    p,
                    match field {
                        SheetActionField::RangeCells | SheetActionField::RadiusCells => "integer",
                        SheetActionField::DamageDice | SheetActionField::DamageType => "string",
                    },
                )
            })
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn dice_only(dice: &dnd_dice::Expr) -> String {
    let mut text = String::from("0");
    for term in dice.terms() {
        if let dnd_dice::Term::Dice(d) = &term.term {
            text.push(if term.sign == dnd_dice::Sign::Plus {
                '+'
            } else {
                '-'
            });
            text.push_str(&d.to_string());
        }
    }
    text
}

fn instantiate(
    rules: &SystemRulesDto,
    list: &SheetActionCollection,
    index: usize,
    version: Version,
    row: &Value,
) -> Option<PluginAction> {
    let chosen = list
        .action_field
        .as_ref()
        .and_then(|p| at(row, p))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| list.actions.first().map(String::as_str))?;
    if !list.actions.iter().any(|a| a == chosen) {
        return None;
    }
    let mut a = rules
        .mechanics
        .actions
        .iter()
        .find(|a| a.id == chosen)?
        .clone();
    a.id = format!("sheet:{}:{index}:{}", list.id, version.0);
    if let Some(name) = at(row, &list.name_field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if name.chars().count() > 100 {
            return None;
        }
        a.label = Label::Text(name.to_owned());
    }
    if let Some(formula) = &list.attack_dice {
        a.attack_dice.as_ref()?;
        let dice = dnd_dice::Expr::parse(formula).ok()?;
        let mut text = String::from("0");
        for term in dice.terms() {
            text.push(if term.sign == dnd_dice::Sign::Plus {
                '+'
            } else {
                '-'
            });
            if let dnd_dice::Term::Field(field) = &term.term
                && let Some(field) = field.strip_prefix("row.")
            {
                let n = at(row, field)?.as_i64()?;
                if !(-1000..=1000).contains(&n) {
                    return None;
                }
                // Normalize signs before parsing (the dice grammar accepts a signed sum).
                if n < 0 {
                    text.pop();
                    text.push(if term.sign == dnd_dice::Sign::Plus {
                        '-'
                    } else {
                        '+'
                    });
                }
                text.push_str(&n.abs().to_string());
            } else {
                text.push_str(&term.term.to_string());
            }
        }
        a.attack_dice = Some(dnd_dice::Expr::parse(&text).ok()?.to_string());
    }
    for (field, p) in &list.bindings {
        let Some(value) = at(row, p) else {
            continue;
        };
        match field {
            SheetActionField::DamageDice => {
                let text = value.as_str()?.trim();
                if text.is_empty() {
                    continue;
                }
                let dice = dnd_dice::Expr::parse(text).ok()?;
                // A maintained effect cannot silently become damage through a sheet edit.
                if (a.maintain.is_some() || a.turn_condition.is_some()) && text != "0" {
                    return None;
                }
                a.damage_dice = dice.to_string();
                if a.critical_if.is_some() {
                    a.critical_dice = Some(dice_only(&dice));
                }
            }
            SheetActionField::RangeCells => {
                let n = value.as_i64()?;
                if !(0..=1000).contains(&n) {
                    return None;
                }
                a.range_cells = n.to_string();
            }
            SheetActionField::RadiusCells => {
                let n = value.as_u64()?;
                if n > 100 {
                    return None;
                }
                a.radius_cells = (n > 0).then_some(n as u16);
                if a.radius_cells.is_none() {
                    a.spatial.area_line_of_effect = false;
                }
            }
            SheetActionField::DamageType => {
                let text = value.as_str()?;
                if text.is_empty() {
                    continue;
                }
                if !ident(text) {
                    return None;
                }
                a.damage_type = Some(text.to_owned());
            }
        }
    }
    Some(a)
}

pub(crate) fn rows(state: &GameState, token: &TokenDto) -> Vec<SheetActionView> {
    let Some(rules) = state.rules() else {
        return Vec::new();
    };
    let Some(c) = token.character_id.and_then(|id| state.characters.get(&id)) else {
        return Vec::new();
    };
    let Ok(data) = serde_json::from_str::<Value>(&c.sheet_json) else {
        return Vec::new();
    };
    rules
        .mechanics
        .sheet_actions
        .iter()
        .flat_map(|list| {
            at(&data, &list.path)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .take(100)
                .enumerate()
                .filter_map(|(index, row)| {
                    Some(SheetActionView {
                        collection: list.id.clone(),
                        index: index as u16,
                        action: instantiate(rules, list, index, c.version, row)?,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(crate) fn resolve(state: &GameState, source: TokenId, id: &str) -> Option<PluginAction> {
    if let Some(a) = state.rules()?.mechanics.actions.iter().find(|a| a.id == id) {
        return Some(a.clone());
    }
    rows(state, state.tokens.get(&source)?)
        .into_iter()
        .find(|v| v.action.id == id)
        .map(|v| v.action)
}
