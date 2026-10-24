//! Evaluation: exact, deterministic, short-circuiting `&&`, `||` and `if`.

use crate::ast::{BinaryOp, Expr, Function, Node, NodeKind, UnaryOp};
use crate::context::{Context, LookupError};
use crate::error::ExprError;
use crate::value::{Rational, Value, ValueType};

impl Expr {
    /// Evaluates the expression; `@` references are looked up in `context`.
    ///
    /// `&&`, `||` and `if` evaluate only what they need, so an untaken branch may divide by zero
    /// or reference a missing field.
    ///
    /// # Errors
    /// Type errors, division by zero, overflow and failed references, see [`ExprError`].
    pub fn eval<C: Context + ?Sized>(&self, context: &C) -> Result<Value, ExprError> {
        eval(&self.root, context)
    }

    /// [`Expr::eval`] followed by [`Value::as_integer`].
    ///
    /// # Errors
    /// See [`Expr::eval`]; [`ExprError::NotAnInteger`] for fractions and booleans.
    pub fn eval_integer<C: Context + ?Sized>(&self, context: &C) -> Result<i64, ExprError> {
        self.eval(context)?.as_integer()
    }
}

fn number(value: Value, offset: usize) -> Result<Rational, ExprError> {
    value.as_number().ok_or(ExprError::TypeMismatch {
        offset,
        expected: ValueType::Number,
        found: value.value_type(),
    })
}

fn boolean(value: Value, offset: usize) -> Result<bool, ExprError> {
    value.as_bool().ok_or(ExprError::TypeMismatch {
        offset,
        expected: ValueType::Bool,
        found: value.value_type(),
    })
}

/// `x op rhs`; `rhs` is evaluated only when needed (`&&`, `||` short-circuit).
fn binary(
    op: BinaryOp,
    x: Value,
    rhs: impl FnOnce() -> Result<Value, ExprError>,
    offset: usize,
) -> Result<Value, ExprError> {
    let numbers = |y: Value| Ok::<_, ExprError>((number(x, offset)?, number(y, offset)?));
    let n = |r: Option<Rational>| r.map(Value::Number).ok_or(ExprError::Overflow { offset });
    match op {
        BinaryOp::And => Ok(Value::Bool(boolean(x, offset)? && boolean(rhs()?, offset)?)),
        BinaryOp::Or => Ok(Value::Bool(boolean(x, offset)? || boolean(rhs()?, offset)?)),
        BinaryOp::Eq | BinaryOp::Ne => {
            let y = rhs()?;
            if x.value_type() != y.value_type() {
                return Err(ExprError::TypeMismatch {
                    offset,
                    expected: x.value_type(),
                    found: y.value_type(),
                });
            }
            Ok(Value::Bool((x == y) == (op == BinaryOp::Eq)))
        }
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
            let (x, y) = numbers(rhs()?)?;
            Ok(Value::Bool(match op {
                BinaryOp::Lt => x < y,
                BinaryOp::Le => x <= y,
                BinaryOp::Gt => x > y,
                _ => x >= y,
            }))
        }
        BinaryOp::Add => {
            let (x, y) = numbers(rhs()?)?;
            n(x.checked_add(y))
        }
        BinaryOp::Sub => {
            let (x, y) = numbers(rhs()?)?;
            n(x.checked_sub(y))
        }
        BinaryOp::Mul => {
            let (x, y) = numbers(rhs()?)?;
            n(x.checked_mul(y))
        }
        BinaryOp::Div => {
            let (x, y) = numbers(rhs()?)?;
            if y == Rational::ZERO {
                return Err(ExprError::DivisionByZero { offset });
            }
            n(x.checked_div(y))
        }
        BinaryOp::Rem => {
            let (x, y) = numbers(rhs()?)?;
            if !(x.is_integer() && y.is_integer()) {
                return Err(ExprError::IntegerRequired { offset });
            }
            if y == Rational::ZERO {
                return Err(ExprError::DivisionByZero { offset });
            }
            n(x.checked_rem(y))
        }
    }
}

fn eval<C: Context + ?Sized>(node: &Node, cx: &C) -> Result<Value, ExprError> {
    let offset = node.offset;
    let overflow = ExprError::Overflow { offset };
    match &node.kind {
        NodeKind::Integer(n) => Ok(Value::from(*n)),
        NodeKind::Bool(b) => Ok(Value::Bool(*b)),
        NodeKind::Ref(path) => cx.lookup(path).map_err(|e| match e {
            LookupError::Unknown => ExprError::UnknownReference {
                offset,
                path: path.clone(),
            },
            LookupError::Other(message) => ExprError::Reference {
                offset,
                path: path.clone(),
                message,
            },
        }),
        NodeKind::Unary(op, x) => {
            let v = eval(x, cx)?;
            match op {
                UnaryOp::Neg => number(v, offset)?
                    .checked_neg()
                    .map(Value::Number)
                    .ok_or(overflow),
                UnaryOp::Not => Ok(Value::Bool(!boolean(v, offset)?)),
            }
        }
        NodeKind::Chain(first, links) => {
            let mut acc = eval(first, cx)?;
            for link in links {
                acc = binary(link.op, acc, || eval(&link.rhs, cx), link.offset)?;
            }
            Ok(acc)
        }
        NodeKind::If(c, a, b) => {
            if boolean(eval(c, cx)?, c.offset)? {
                eval(a, cx)
            } else {
                eval(b, cx)
            }
        }
        NodeKind::Call(function, args) => {
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                values.push(number(eval(arg, cx)?, arg.offset)?);
            }
            let first = values.first().copied().unwrap_or(Rational::ZERO);
            let result = match function {
                Function::Min => values.into_iter().min(),
                Function::Max => values.into_iter().max(),
                Function::Floor => Some(first.floor()),
                Function::Ceil => first.checked_ceil(),
                Function::Round => first.checked_round(),
                Function::Abs => first.checked_abs(),
            };
            result.map(Value::Number).ok_or(overflow)
        }
    }
}
