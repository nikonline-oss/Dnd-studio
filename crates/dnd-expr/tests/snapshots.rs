#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::collections::BTreeMap;
use std::fmt::Write as _;

use dnd_expr::{Expr, ExprError, Value};

/// One line per input: `input => code @ offset: message`.
fn report(inputs: &[&str], run: impl Fn(&str) -> Result<Value, ExprError>) -> String {
    let mut out = String::new();
    for input in inputs {
        let line = match run(input) {
            Ok(v) => format!("ok: {v}"),
            Err(e) => format!(
                "{} @ {}: {e}",
                e.code(),
                e.offset().map_or_else(|| "-".to_owned(), |o| o.to_string())
            ),
        };
        writeln!(out, "{input:?} => {line}").unwrap();
    }
    out
}

#[test]
fn parse_errors() {
    insta::assert_snapshot!(report(
        &[
            "",
            "1 +",
            "(1 + 2",
            "1 2",
            "1 = 2",
            "1 & 2",
            "1 < 2 < 3",
            "1 == 2 != 3",
            "@",
            "@1a",
            "@a.",
            "str + 1",
            "sqrt(4)",
            "min()",
            "floor(1, 2)",
            "min(1,)",
            "if true then 1",
            "if true 1 else 2",
            "if then",
            "1d6",
            "99999999999999999999",
            "abs(1",
            "1 + * 2",
            "#",
        ],
        |s| Expr::parse(s).map(|_| Value::Bool(true)),
    ));
}

#[test]
fn eval_errors() {
    let sheet = BTreeMap::from([("str".to_owned(), 15_i64)]);
    insta::assert_snapshot!(report(
        &[
            "@str / (@str - 15)",
            "@dex + 1",
            "@str + true",
            "if @str then 1 else 2",
            "@str > 10 && @str",
            "(@str / 2) % 2",
            "9223372036854775807 + @str",
            "floor(@str / 2) + round(@str / 2)",
        ],
        |s| Expr::parse(s)?.eval(&sheet),
    ));
}
