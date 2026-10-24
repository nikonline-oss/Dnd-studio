//! Syntax tree, canonical rendering and serde.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize, Serializer};

use crate::error::ExprError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinaryOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl BinaryOp {
    pub(crate) const fn symbol(self) -> &'static str {
        match self {
            Self::Or => "||",
            Self::And => "&&",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Rem => "%",
        }
    }

    pub(crate) const fn precedence(self) -> u8 {
        match self {
            Self::Or => PREC_OR,
            Self::And => PREC_AND,
            Self::Eq | Self::Ne | Self::Lt | Self::Le | Self::Gt | Self::Ge => PREC_CMP,
            Self::Add | Self::Sub => PREC_ADD,
            Self::Mul | Self::Div | Self::Rem => PREC_MUL,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Function {
    Min,
    Max,
    Floor,
    Ceil,
    Round,
    Abs,
}

impl Function {
    pub(crate) const ALL: [Self; 6] = [
        Self::Min,
        Self::Max,
        Self::Floor,
        Self::Ceil,
        Self::Round,
        Self::Abs,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Min => "min",
            Self::Max => "max",
            Self::Floor => "floor",
            Self::Ceil => "ceil",
            Self::Round => "round",
            Self::Abs => "abs",
        }
    }

    /// Whether `n` arguments are accepted, and how to describe the accepted count.
    pub(crate) const fn arity(self, n: usize) -> (bool, &'static str) {
        match self {
            Self::Min | Self::Max => (n >= 1, "1 or more"),
            Self::Floor | Self::Ceil | Self::Round | Self::Abs => (n == 1, "1"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NodeKind {
    Integer(i64),
    Bool(bool),
    Ref(String),
    Unary(UnaryOp, Box<Node>),
    /// `first op rhs op rhs …`, left-associative; all operators share one precedence level.
    /// Comparisons have exactly one link. Chains keep tree depth bounded by syntactic nesting,
    /// so `1 + 1 + … + 1` does not recurse once per term.
    Chain(Box<Node>, Vec<Link>),
    If(Box<Node>, Box<Node>, Box<Node>),
    Call(Function, Vec<Node>),
}

/// A syntax node. `offset` is the byte offset of the operator for unary and binary nodes and of
/// the first character otherwise; it is ignored by equality.
#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub(crate) kind: NodeKind,
    pub(crate) offset: usize,
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl Eq for Node {}

/// One `op rhs` step of a [`NodeKind::Chain`]; `offset` is the operator's, ignored by equality.
#[derive(Debug, Clone)]
pub(crate) struct Link {
    pub(crate) op: BinaryOp,
    pub(crate) offset: usize,
    pub(crate) rhs: Node,
}

impl PartialEq for Link {
    fn eq(&self, other: &Self) -> bool {
        self.op == other.op && self.rhs == other.rhs
    }
}

impl Eq for Link {}

const PREC_IF: u8 = 0;
const PREC_OR: u8 = 1;
const PREC_AND: u8 = 2;
const PREC_CMP: u8 = 3;
const PREC_ADD: u8 = 4;
const PREC_MUL: u8 = 5;
const PREC_UNARY: u8 = 6;
const PREC_ATOM: u8 = 7;

impl Node {
    pub(crate) fn precedence(&self) -> u8 {
        match &self.kind {
            NodeKind::If(..) => PREC_IF,
            NodeKind::Chain(_, links) => links.first().map_or(PREC_ATOM, |l| l.op.precedence()),
            NodeKind::Unary(..) => PREC_UNARY,
            NodeKind::Integer(_) | NodeKind::Bool(_) | NodeKind::Ref(_) | NodeKind::Call(..) => {
                PREC_ATOM
            }
        }
    }

    fn write_child(&self, f: &mut fmt::Formatter<'_>, parens: bool) -> fmt::Result {
        if parens {
            write!(f, "({self})")
        } else {
            write!(f, "{self}")
        }
    }

    fn collect_refs<'a>(&'a self, out: &mut Vec<(&'a str, usize)>) {
        match &self.kind {
            NodeKind::Integer(_) | NodeKind::Bool(_) => {}
            NodeKind::Ref(path) => out.push((path, self.offset)),
            NodeKind::Unary(_, x) => x.collect_refs(out),
            NodeKind::Chain(first, links) => {
                first.collect_refs(out);
                links.iter().for_each(|l| l.rhs.collect_refs(out));
            }
            NodeKind::If(c, a, b) => {
                c.collect_refs(out);
                a.collect_refs(out);
                b.collect_refs(out);
            }
            NodeKind::Call(_, args) => args.iter().for_each(|a| a.collect_refs(out)),
        }
    }
}

/// Canonical form: single spaces around binary operators and keywords, `f(a, b)`, and only the
/// parentheses precedence requires (an `if` inside an operator is always parenthesized).
impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            NodeKind::Integer(n) => write!(f, "{n}"),
            NodeKind::Bool(b) => write!(f, "{b}"),
            NodeKind::Ref(path) => write!(f, "@{path}"),
            NodeKind::Unary(op, x) => {
                f.write_str(match op {
                    UnaryOp::Neg => "-",
                    UnaryOp::Not => "!",
                })?;
                x.write_child(f, x.precedence() < PREC_UNARY)
            }
            NodeKind::Chain(first, links) => {
                let p = self.precedence();
                // Left-associative (a same-level left operand is flattened by the parser);
                // comparisons do not associate at all.
                let left = first.precedence() < p || (p == PREC_CMP && first.precedence() == p);
                first.write_child(f, left)?;
                for link in links {
                    write!(f, " {} ", link.op.symbol())?;
                    link.rhs.write_child(f, link.rhs.precedence() <= p)?;
                }
                Ok(())
            }
            NodeKind::If(c, a, b) => write!(f, "if {c} then {a} else {b}"),
            NodeKind::Call(func, args) => {
                write!(f, "{}(", func.name())?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                f.write_str(")")
            }
        }
    }
}

/// A parsed expression.
///
/// Equality is structural: the same tree written differently (`1+2` and `(1) + 2`) is equal.
/// [`fmt::Display`] renders the canonical form; serde keeps the source text as written, so error
/// offsets keep pointing into what the author wrote (the canonical form may be longer than the
/// source and so exceed [`crate::limits::MAX_EXPR_LEN`]).
#[derive(Debug, Clone)]
pub struct Expr {
    pub(crate) root: Node,
    pub(crate) source: String,
}

impl Expr {
    /// Parses and validates an expression (syntax, function arity, limits).
    ///
    /// # Errors
    /// Syntax and limit errors, see [`ExprError`].
    pub fn parse(input: &str) -> Result<Self, ExprError> {
        crate::parse::parse(input)
    }

    /// The text this expression was parsed from.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Distinct field paths (without `@`) the expression references, sorted.
    #[must_use]
    pub fn references(&self) -> BTreeSet<&str> {
        self.references_with_offsets()
            .into_iter()
            .map(|(p, _)| p)
            .collect()
    }

    /// Every reference with its byte offset, in source order.
    pub(crate) fn references_with_offsets(&self) -> Vec<(&str, usize)> {
        let mut out = Vec::new();
        self.root.collect_refs(&mut out);
        out
    }
}

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
    }
}

impl Eq for Expr {}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.root.fmt(f)
    }
}

impl FromStr for Expr {
    type Err = ExprError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for Expr {
    type Error = ExprError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::parse(&s)
    }
}

impl Serialize for Expr {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.source)
    }
}

impl<'de> Deserialize<'de> for Expr {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}
