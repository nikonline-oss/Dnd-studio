#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::collections::BTreeMap;

use dnd_expr::limits::MAX_EXPR_LEN;
use dnd_expr::{Expr, ExprError, Rational, Value};
use proptest::prelude::*;

fn sheet() -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("a".to_owned(), Value::from(3)),
        ("b.c".to_owned(), Value::from(-7)),
        ("big".to_owned(), Value::from(i64::MAX)),
        ("f".to_owned(), Value::from(Rational::new(-5, 2).unwrap())),
        ("t".to_owned(), Value::from(true)),
    ])
}

/// Random but mostly well-formed source text (sometimes ill-typed, sometimes failing at runtime).
fn source() -> impl Strategy<Value = String> {
    let leaf = prop_oneof![
        (0i64..=20).prop_map(|n| n.to_string()),
        Just(i64::MAX.to_string()),
        Just("true".to_owned()),
        Just("false".to_owned()),
        prop_oneof![
            Just("@a"),
            Just("@b.c"),
            Just("@big"),
            Just("@f"),
            Just("@t")
        ]
        .prop_map(str::to_owned),
    ];
    leaf.prop_recursive(5, 48, 4, |inner| {
        let ops = prop_oneof![
            Just("+"),
            Just("-"),
            Just("*"),
            Just("/"),
            Just("%"),
            Just("<"),
            Just("<="),
            Just(">"),
            Just(">="),
            Just("=="),
            Just("!="),
            Just("&&"),
            Just("||"),
        ];
        let funcs = prop_oneof![Just("floor"), Just("ceil"), Just("round"), Just("abs"),];
        prop_oneof![
            // Comparisons do not chain, so they are always parenthesized.
            (inner.clone(), ops, inner.clone()).prop_map(|(a, op, b)| {
                if ["<", "<=", ">", ">=", "==", "!="].contains(&op) {
                    format!("({a}{op}{b})")
                } else {
                    format!("{a}{op}{b}")
                }
            }),
            (inner.clone(), any::<bool>()).prop_map(|(a, paren)| if paren {
                format!("({a})")
            } else {
                format!(" {a} ")
            }),
            (prop_oneof![Just("-"), Just("!")], inner.clone())
                .prop_map(|(op, a)| format!("{op}{a}")),
            (inner.clone(), inner.clone(), inner.clone())
                .prop_map(|(c, a, b)| format!("(if {c} then {a} else {b})")),
            (
                prop_oneof![Just("min"), Just("max")],
                prop::collection::vec(inner.clone(), 1..4)
            )
                .prop_map(|(f, args)| format!("{f}({})", args.join(","))),
            (funcs, inner).prop_map(|(f, a)| format!("{f}({a})")),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn arbitrary_input_never_panics(input in "\\PC{0,64}") {
        let _ = Expr::parse(&input).map(|e| e.eval(&sheet()));
    }

    #[test]
    fn grammar_soup_never_panics(
        input in prop::collection::vec(
            prop_oneof![
                Just("("), Just(")"), Just("@a"), Just("1"), Just("if"), Just("then"),
                Just("else"), Just("min("), Just(","), Just("-"), Just("!"), Just("<"),
                Just("=="), Just("&&"), Just(" "), Just("true"), Just("/"), Just("0"),
            ],
            0..80,
        ).prop_map(|parts| parts.concat())
    ) {
        if let Ok(expr) = Expr::parse(&input) {
            let _ = expr.eval(&sheet());
            prop_assert_eq!(Expr::parse(&expr.to_string()).unwrap(), expr);
        }
    }

    #[test]
    fn long_input_is_rejected_by_length(n in (MAX_EXPR_LEN + 1)..(MAX_EXPR_LEN * 3)) {
        let input = "1+".repeat(n / 2 + 1);
        let too_long = matches!(Expr::parse(&input), Err(ExprError::TooLong { .. }));
        prop_assert!(too_long);
    }

    #[test]
    fn canonical_form_roundtrips(src in source()) {
        prop_assume!(src.chars().count() <= MAX_EXPR_LEN);
        let expr = Expr::parse(&src).unwrap();
        let canonical = expr.to_string();
        let again = Expr::parse(&canonical).unwrap();
        prop_assert_eq!(&again, &expr);
        prop_assert_eq!(again.to_string(), canonical);
        // Same meaning: the same value or the same kind of error.
        let (x, y) = (expr.eval(&sheet()), again.eval(&sheet()));
        prop_assert_eq!(x.as_ref().ok(), y.as_ref().ok());
        prop_assert_eq!(x.err().map(|e| e.code()), y.err().map(|e| e.code()));
        prop_assert_eq!(expr.references(), again.references());
        let json = serde_json::to_string(&expr).unwrap();
        prop_assert_eq!(serde_json::from_str::<Expr>(&json).unwrap(), expr);
    }

    #[test]
    fn eval_is_deterministic(src in source()) {
        prop_assume!(src.chars().count() <= MAX_EXPR_LEN);
        let expr = Expr::parse(&src).unwrap();
        prop_assert_eq!(expr.eval(&sheet()), expr.eval(&sheet()));
        prop_assert_eq!(expr.eval(&sheet()), Expr::parse(&src).unwrap().eval(&sheet()));
    }

    #[test]
    fn floor_ceil_round_bracket_the_value(n in any::<i64>(), d in 1i64..=1000) {
        let ctx = BTreeMap::from([("n".to_owned(), n), ("d".to_owned(), d)]);
        let int = |f: &str| Expr::parse(&format!("{f}(@n / @d)")).unwrap().eval_integer(&ctx);
        let (fl, ce, ro) = (int("floor").unwrap(), int("ceil").unwrap(), int("round").unwrap());
        let exact = i128::from(n);
        let d = i128::from(d);
        prop_assert!(i128::from(fl) * d <= exact && exact < (i128::from(fl) + 1) * d);
        prop_assert!(i128::from(ce) - i128::from(fl) <= 1);
        prop_assert!(ro == fl || ro == ce);
        // Halves go away from zero; otherwise round is the nearest.
        let twice = 2 * (exact - i128::from(fl) * d); // 2 * remainder, compared to d
        let want = if twice > d || (twice == d && n > 0) { ce } else { fl };
        prop_assert_eq!(ro, if twice == 0 { fl } else { want });
    }
}
