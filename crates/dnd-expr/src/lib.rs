//! Safe minimal expression language for rule-system plugins (ADR-0005).
//!
//! ```
//! use std::collections::BTreeMap;
//! use dnd_expr::{Expr, Value};
//!
//! let sheet = BTreeMap::from([("str".to_owned(), 15_i64)]);
//! let expr = Expr::parse("floor((@str - 10) / 2)")?;
//! assert_eq!(expr.eval(&sheet)?, Value::from(2));
//! assert_eq!(expr.to_string(), "floor((@str - 10) / 2)");
//! # Ok::<(), dnd_expr::ExprError>(())
//! ```
//!
//! See `README.md` for the grammar, semantics, limits and dependency boundaries.

mod ast;
mod computed;
mod context;
mod error;
mod eval;
mod parse;
mod value;

pub mod limits {
    //! Hard limits; every [`crate::Expr`] satisfies them.

    /// Maximum expression length in characters.
    pub const MAX_EXPR_LEN: usize = 1024;
    /// Maximum syntactic nesting: parentheses, function calls, `if` and unary operators.
    pub const MAX_DEPTH: usize = 32;
}

pub use ast::Expr;
pub use computed::{Computed, Scope};
pub use context::{Context, LookupError, NoContext};
pub use error::ExprError;
pub use value::{Rational, Value, ValueType};
