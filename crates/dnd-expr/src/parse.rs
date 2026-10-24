//! winnow grammar of the expression language (ADR-0005) with limit checks.
//!
//! ```text
//! expr    = or
//! or      = and { "||" and }
//! and     = cmp { "&&" cmp }
//! cmp     = add [ ("==" | "!=" | "<=" | "<" | ">=" | ">") add ]      (does not chain)
//! add     = mul { ("+" | "-") mul }
//! mul     = unary { ("*" | "/" | "%") unary }
//! unary   = ("-" | "!") unary | primary
//! primary = integer | "true" | "false" | "@" path | "(" expr ")"
//!         | "if" expr "then" expr "else" expr
//!         | name "(" [ expr { "," expr } ] ")"
//! path    = ident { "." ident },  ident = [A-Za-z_][A-Za-z0-9_]*
//! ```
//!
//! Whitespace is allowed between all tokens. Every parenthesis, function call, `if` and unary
//! operator adds a nesting level (max [`MAX_DEPTH`]); binary chains are parsed iteratively.
//! Semantic errors (too deep, unknown function, arity, …) are raised as the winnow error's cause
//! so they keep their own type and offset.

use winnow::ascii::{digit1, multispace0};
use winnow::combinator::{alt, cut_err, eof, fail, opt, separated};
use winnow::error::{ContextError, ErrMode, FromExternalError, StrContext, StrContextValue};
use winnow::prelude::*;
use winnow::stream::{LocatingSlice, Location, Stream};
use winnow::token::{one_of, take_while};

use crate::ast::{BinaryOp, Expr, Function, Link, Node, NodeKind, UnaryOp};
use crate::error::ExprError;
use crate::limits::{MAX_DEPTH, MAX_EXPR_LEN};

type In<'i> = LocatingSlice<&'i str>;

fn expected(what: &'static str) -> StrContext {
    StrContext::Expected(StrContextValue::Description(what))
}

fn literal(what: &'static str) -> StrContext {
    StrContext::Expected(StrContextValue::StringLiteral(what))
}

fn raise<T>(i: &In<'_>, error: ExprError) -> ModalResult<T> {
    Err(ErrMode::Cut(ContextError::from_external_error(i, error)))
}

fn ws(i: &mut In<'_>) -> ModalResult<()> {
    multispace0.void().parse_next(i)
}

fn nest(i: &In<'_>, depth: usize) -> ModalResult<usize> {
    if depth >= MAX_DEPTH {
        return raise(
            i,
            ExprError::TooDeep {
                offset: i.current_token_start(),
            },
        );
    }
    Ok(depth + 1)
}

fn ident<'i>(i: &mut In<'i>) -> ModalResult<&'i str> {
    (
        one_of(|c: char| c.is_ascii_alphabetic() || c == '_'),
        take_while(0.., |c: char| c.is_ascii_alphanumeric() || c == '_'),
    )
        .take()
        .parse_next(i)
}

fn path<'i>(i: &mut In<'i>) -> ModalResult<&'i str> {
    separated::<_, _, (), _, _, _, _>(1.., ident, '.')
        .take()
        .parse_next(i)
}

/// Whether `name` is a valid field path (`name`, `name.sub`).
pub(crate) fn is_valid_path(name: &str) -> bool {
    (path, eof).parse(LocatingSlice::new(name)).is_ok()
}

fn keyword<'i>(kw: &'static str) -> impl Parser<In<'i>, &'i str, ErrMode<ContextError>> {
    ident.verify(move |w: &str| w == kw).context(literal(kw))
}

fn expr(i: &mut In<'_>, depth: usize) -> ModalResult<Node> {
    binary_level(i, depth, and, |i| "||".value(BinaryOp::Or).parse_next(i))
}

fn and(i: &mut In<'_>, depth: usize) -> ModalResult<Node> {
    binary_level(i, depth, cmp, |i| "&&".value(BinaryOp::And).parse_next(i))
}

fn cmp_op(i: &mut In<'_>) -> ModalResult<BinaryOp> {
    alt((
        "==".value(BinaryOp::Eq),
        "!=".value(BinaryOp::Ne),
        "<=".value(BinaryOp::Le),
        "<".value(BinaryOp::Lt),
        ">=".value(BinaryOp::Ge),
        ">".value(BinaryOp::Gt),
    ))
    .parse_next(i)
}

fn cmp(i: &mut In<'_>, depth: usize) -> ModalResult<Node> {
    let left = add(i, depth)?;
    let start = i.checkpoint();
    ws(i)?;
    let offset = i.current_token_start();
    let Some(op) = opt(cmp_op).parse_next(i)? else {
        i.reset(&start);
        return Ok(left);
    };
    ws(i)?;
    let rhs = cut_err(|i: &mut In<'_>| add(i, depth)).parse_next(i)?;
    let after = i.checkpoint();
    ws(i)?;
    let next = i.current_token_start();
    if opt(cmp_op).parse_next(i)?.is_some() {
        return raise(i, ExprError::ChainedComparison { offset: next });
    }
    i.reset(&after);
    Ok(Node {
        kind: NodeKind::Chain(Box::new(left), vec![Link { op, offset, rhs }]),
        offset,
    })
}

fn add(i: &mut In<'_>, depth: usize) -> ModalResult<Node> {
    binary_level(i, depth, mul, |i| {
        alt(('+'.value(BinaryOp::Add), '-'.value(BinaryOp::Sub))).parse_next(i)
    })
}

fn mul(i: &mut In<'_>, depth: usize) -> ModalResult<Node> {
    binary_level(i, depth, unary, |i| {
        alt((
            '*'.value(BinaryOp::Mul),
            '/'.value(BinaryOp::Div),
            '%'.value(BinaryOp::Rem),
        ))
        .parse_next(i)
    })
}

/// A left-associative chain `next { op next }`. A parenthesized chain of the same level as the
/// first operand is flattened (`(a + b) - c` is `a + b - c`), which keeps the canonical form a
/// fixed point.
fn binary_level<'i>(
    i: &mut In<'i>,
    depth: usize,
    next: fn(&mut In<'i>, usize) -> ModalResult<Node>,
    op: fn(&mut In<'i>) -> ModalResult<BinaryOp>,
) -> ModalResult<Node> {
    let first = next(i, depth)?;
    let mut links = Vec::new();
    loop {
        let start = i.checkpoint();
        ws(i)?;
        let offset = i.current_token_start();
        let Some(op) = opt(op).parse_next(i)? else {
            i.reset(&start);
            break;
        };
        ws(i)?;
        let rhs = cut_err(|i: &mut In<'i>| next(i, depth)).parse_next(i)?;
        links.push(Link { op, offset, rhs });
    }
    let Some(level) = links.first().map(|l| l.op.precedence()) else {
        return Ok(first);
    };
    match first {
        Node {
            kind: NodeKind::Chain(inner, mut inner_links),
            offset,
        } if first_precedence(&inner_links) == Some(level) => {
            inner_links.append(&mut links);
            Ok(Node {
                kind: NodeKind::Chain(inner, inner_links),
                offset,
            })
        }
        first => {
            let offset = links.first().map_or(first.offset, |l| l.offset);
            Ok(Node {
                kind: NodeKind::Chain(Box::new(first), links),
                offset,
            })
        }
    }
}

fn first_precedence(links: &[Link]) -> Option<u8> {
    links.first().map(|l| l.op.precedence())
}

fn unary(i: &mut In<'_>, depth: usize) -> ModalResult<Node> {
    let offset = i.current_token_start();
    let op = opt(alt(('-'.value(UnaryOp::Neg), '!'.value(UnaryOp::Not)))).parse_next(i)?;
    let Some(op) = op else {
        return primary(i, depth);
    };
    let depth = nest(i, depth)?;
    ws(i)?;
    let operand = cut_err(|i: &mut In<'_>| unary(i, depth)).parse_next(i)?;
    Ok(Node {
        kind: NodeKind::Unary(op, Box::new(operand)),
        offset,
    })
}

fn primary(i: &mut In<'_>, depth: usize) -> ModalResult<Node> {
    let offset = i.current_token_start();
    let node = |kind| Node { kind, offset };
    match i.peek_token() {
        Some('(') => {
            let depth = nest(i, depth)?;
            '('.parse_next(i)?;
            ws(i)?;
            let inner = cut_err(|i: &mut In<'_>| expr(i, depth)).parse_next(i)?;
            ws(i)?;
            cut_err(')'.context(literal(")"))).parse_next(i)?;
            Ok(inner)
        }
        Some('@') => {
            '@'.parse_next(i)?;
            let p = cut_err(path.context(expected("field name"))).parse_next(i)?;
            Ok(node(NodeKind::Ref(p.to_owned())))
        }
        Some(c) if c.is_ascii_digit() => {
            let digits = digit1.parse_next(i)?;
            match digits.parse::<i64>() {
                Ok(n) => Ok(node(NodeKind::Integer(n))),
                Err(_) => raise(i, ExprError::IntegerOutOfRange { offset }),
            }
        }
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            let name = ident.parse_next(i)?;
            match name {
                "true" => Ok(node(NodeKind::Bool(true))),
                "false" => Ok(node(NodeKind::Bool(false))),
                "if" => if_else(i, depth, offset),
                "then" | "else" => raise(
                    i,
                    ExprError::Syntax {
                        offset,
                        message: format!("expected an expression, found `{name}`"),
                    },
                ),
                _ => call(i, depth, offset, name),
            }
        }
        _ => fail
            .context(expected(
                "number, @field, `(`, function call, `if`, `true` or `false`",
            ))
            .parse_next(i),
    }
}

fn if_else(i: &mut In<'_>, depth: usize, offset: usize) -> ModalResult<Node> {
    let depth = nest(i, depth)?;
    let branch = |i: &mut In<'_>| -> ModalResult<Node> {
        ws(i)?;
        let e = cut_err(|i: &mut In<'_>| expr(i, depth)).parse_next(i)?;
        ws(i)?;
        Ok(e)
    };
    let cond = branch(i)?;
    cut_err(keyword("then")).parse_next(i)?;
    let then = branch(i)?;
    cut_err(keyword("else")).parse_next(i)?;
    let otherwise = branch(i)?;
    Ok(Node {
        kind: NodeKind::If(Box::new(cond), Box::new(then), Box::new(otherwise)),
        offset,
    })
}

fn call(i: &mut In<'_>, depth: usize, offset: usize, name: &str) -> ModalResult<Node> {
    let start = i.checkpoint();
    ws(i)?;
    if i.peek_token() != Some('(') {
        i.reset(&start);
        return raise(
            i,
            ExprError::UnknownName {
                offset,
                name: name.to_owned(),
            },
        );
    }
    let Some(function) = Function::ALL.into_iter().find(|f| f.name() == name) else {
        return raise(
            i,
            ExprError::UnknownFunction {
                offset,
                name: name.to_owned(),
            },
        );
    };
    let depth = nest(i, depth)?;
    '('.parse_next(i)?;
    ws(i)?;
    let args: Vec<Node> = if i.peek_token() == Some(')') {
        Vec::new()
    } else {
        separated(
            1..,
            |i: &mut In<'_>| -> ModalResult<Node> {
                ws(i)?;
                let e = cut_err(|i: &mut In<'_>| expr(i, depth)).parse_next(i)?;
                ws(i)?;
                Ok(e)
            },
            ',',
        )
        .parse_next(i)?
    };
    cut_err(')'.context(expected("`,` or `)`"))).parse_next(i)?;
    let (ok, expected) = function.arity(args.len());
    if !ok {
        return raise(
            i,
            ExprError::WrongArity {
                offset,
                function: function.name(),
                expected,
                found: args.len(),
            },
        );
    }
    Ok(Node {
        kind: NodeKind::Call(function, args),
        offset,
    })
}

pub(crate) fn parse(input: &str) -> Result<Expr, ExprError> {
    let len = input.chars().count();
    if len > MAX_EXPR_LEN {
        return Err(ExprError::TooLong { len });
    }
    if input.trim().is_empty() {
        return Err(ExprError::Empty);
    }
    let root = (
        ws,
        |i: &mut In<'_>| expr(i, 0),
        ws,
        cut_err(eof.context(expected("operator or end of expression"))),
    )
        .map(|(_, e, _, _)| e)
        .parse(LocatingSlice::new(input))
        .map_err(|e| {
            match e
                .inner()
                .cause()
                .and_then(|c| c.downcast_ref::<ExprError>())
            {
                Some(err) => err.clone(),
                None => ExprError::Syntax {
                    offset: e.offset(),
                    message: e.inner().to_string().replace('\n', "; "),
                },
            }
        })?;
    Ok(Expr {
        root,
        source: input.to_owned(),
    })
}
