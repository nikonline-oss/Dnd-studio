//! Computed sheet fields: formulas that may reference sheet fields and each other.

use std::collections::BTreeMap;

use crate::ast::Expr;
use crate::context::{Context, LookupError};
use crate::error::ExprError;
use crate::parse::is_valid_path;
use crate::value::Value;

/// Integer values of computed fields, resolved in dependency order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Computed {
    values: BTreeMap<String, i64>,
}

impl Computed {
    /// Evaluates `definitions` (field name → formula) against `sheet`.
    ///
    /// A formula may reference sheet fields and other computed fields; a computed field shadows a
    /// sheet field of the same name. Every reference that is not a computed field must exist in
    /// `sheet` (even in an untaken branch), there must be no cycles, and every result must be an
    /// integer. Errors inside a formula are wrapped in [`ExprError::Computed`].
    ///
    /// # Errors
    /// [`ExprError::InvalidFieldName`], [`ExprError::Cycle`] or [`ExprError::Computed`].
    pub fn resolve<C: Context + ?Sized>(
        definitions: &BTreeMap<String, String>,
        sheet: &C,
    ) -> Result<Self, ExprError> {
        let in_field = |field: &str, source| ExprError::Computed {
            field: field.to_owned(),
            source: Box::new(source),
        };
        let mut exprs = BTreeMap::new();
        for (name, formula) in definitions {
            if !is_valid_path(name) {
                return Err(ExprError::InvalidFieldName { name: name.clone() });
            }
            let expr = Expr::parse(formula).map_err(|e| in_field(name, e))?;
            for (path, offset) in expr.references_with_offsets() {
                if !definitions.contains_key(path)
                    && sheet.lookup(path) == Err(LookupError::Unknown)
                {
                    return Err(in_field(
                        name,
                        ExprError::UnknownReference {
                            offset,
                            path: path.to_owned(),
                        },
                    ));
                }
            }
            exprs.insert(name.as_str(), expr);
        }

        let mut values = BTreeMap::new();
        for name in dependency_order(&exprs)? {
            let Some(expr) = exprs.get(name) else {
                continue;
            };
            let scope = Scope {
                computed: &values,
                sheet,
            };
            let value = expr
                .eval(&scope)
                .and_then(Value::as_integer)
                .map_err(|e| in_field(name, e))?;
            values.insert(name.to_owned(), value);
        }
        Ok(Self { values })
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<i64> {
        self.values.get(name).copied()
    }

    #[must_use]
    pub fn values(&self) -> &BTreeMap<String, i64> {
        &self.values
    }

    #[must_use]
    pub fn into_values(self) -> BTreeMap<String, i64> {
        self.values
    }

    /// A context of these computed values layered over `sheet`, e.g. for evaluating roll macros.
    #[must_use]
    pub fn with_sheet<'a, C: Context + ?Sized>(&'a self, sheet: &'a C) -> Scope<'a, C> {
        Scope {
            computed: &self.values,
            sheet,
        }
    }
}

/// Computed values first, then the sheet. See [`Computed::with_sheet`].
#[derive(Debug, Clone, Copy)]
pub struct Scope<'a, C: ?Sized> {
    computed: &'a BTreeMap<String, i64>,
    sheet: &'a C,
}

impl<C: Context + ?Sized> Context for Scope<'_, C> {
    fn lookup(&self, path: &str) -> Result<Value, LookupError> {
        match self.computed.get(path) {
            Some(&n) => Ok(Value::from(n)),
            None => self.sheet.lookup(path),
        }
    }
}

/// Topological order of computed fields (dependencies first); iterative DFS, deterministic.
fn dependency_order<'a>(exprs: &BTreeMap<&'a str, Expr>) -> Result<Vec<&'a str>, ExprError> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Visiting,
        Done,
    }
    let deps = |name: &str| -> Vec<&'a str> {
        exprs.get(name).map_or_else(Vec::new, |e| {
            e.references()
                .into_iter()
                .filter_map(|r| exprs.get_key_value(r).map(|(k, _)| *k))
                .collect()
        })
    };
    let mut marks: BTreeMap<&str, Mark> = BTreeMap::new();
    let mut order = Vec::with_capacity(exprs.len());
    for &root in exprs.keys() {
        if marks.contains_key(root) {
            continue;
        }
        // Stack of (field, its dependencies, index of the next dependency to visit).
        let mut stack = vec![(root, deps(root), 0_usize)];
        marks.insert(root, Mark::Visiting);
        while let Some((name, children, next)) = stack.last_mut() {
            let Some(&child) = children.get(*next) else {
                marks.insert(*name, Mark::Done);
                order.push(*name);
                stack.pop();
                continue;
            };
            *next += 1;
            match marks.get(child) {
                Some(Mark::Done) => {}
                Some(Mark::Visiting) => {
                    let start = stack.iter().position(|(n, ..)| *n == child).unwrap_or(0);
                    let mut fields: Vec<String> = stack[start..]
                        .iter()
                        .map(|(n, ..)| (*n).to_owned())
                        .collect();
                    fields.push(child.to_owned());
                    return Err(ExprError::Cycle { fields });
                }
                None => {
                    marks.insert(child, Mark::Visiting);
                    stack.push((child, deps(child), 0));
                }
            }
        }
    }
    Ok(order)
}
