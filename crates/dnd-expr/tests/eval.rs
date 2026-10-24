#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::collections::BTreeMap;

use dnd_expr::limits::{MAX_DEPTH, MAX_EXPR_LEN};
use dnd_expr::{
    Computed, Context, Expr, ExprError, LookupError, NoContext, Rational, Value, ValueType,
};

fn sheet() -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("str".to_owned(), Value::from(15)),
        ("hp.max".to_owned(), Value::from(40)),
        ("hp.current".to_owned(), Value::from(9)),
        ("half".to_owned(), Value::from(Rational::new(1, 2).unwrap())),
        ("dead".to_owned(), Value::from(false)),
    ])
}

fn eval(input: &str) -> Result<Value, ExprError> {
    Expr::parse(input)?.eval(&sheet())
}

fn int(input: &str) -> i64 {
    eval(input)
        .unwrap_or_else(|e| panic!("{input}: {e}"))
        .as_integer()
        .unwrap()
}

fn boolean(input: &str) -> bool {
    match eval(input).unwrap_or_else(|e| panic!("{input}: {e}")) {
        Value::Bool(b) => b,
        v => panic!("{input} = {v}, expected a boolean"),
    }
}

fn frac(n: i64, d: i64) -> Value {
    Value::from(Rational::new(n, d).unwrap())
}

#[test]
fn arithmetic() {
    assert_eq!(int("1 + 2 * 3"), 7);
    assert_eq!(int("(1 + 2) * 3"), 9);
    assert_eq!(int("10 - 4 - 3"), 3);
    assert_eq!(int("2 * 3 % 4"), 2);
    assert_eq!(int("-2 * -3"), 6);
    assert_eq!(int("- (2 + 3)"), -5);
    assert_eq!(int("--4"), 4);
    assert_eq!(int("0042"), 42);
    assert_eq!(eval("7 / 2").unwrap(), frac(7, 2));
    assert_eq!(eval("1 / 3 + 1 / 6").unwrap(), frac(1, 2));
    assert_eq!(int("6 / 4 * 2"), 3);
    assert_eq!(int("@str + @hp.max"), 55);
    assert_eq!(eval("@half * 3").unwrap(), frac(3, 2));
}

#[test]
fn remainder_is_floored() {
    assert_eq!(int("7 % 3"), 1);
    assert_eq!(int("-7 % 3"), 2);
    assert_eq!(int("7 % -3"), -2);
    assert_eq!(int("-7 % -3"), -1);
    assert_eq!(int("(4 / 2) % 2"), 0);
    assert_eq!(
        eval("7 % (1 / 2)"),
        Err(ExprError::IntegerRequired { offset: 2 })
    );
    assert_eq!(eval("7 % 0"), Err(ExprError::DivisionByZero { offset: 2 }));
}

#[test]
fn comparisons() {
    assert!(boolean("1 < 2"));
    assert!(!boolean("2 < 2"));
    assert!(boolean("2 <= 2"));
    assert!(boolean("3 > 2"));
    assert!(boolean("2 >= 2"));
    assert!(boolean("1 / 2 == 2 / 4"));
    assert!(boolean("1 / 3 != 1 / 2"));
    assert!(boolean("1 / 3 < 1 / 2"));
    assert!(boolean("true == true"));
    assert!(boolean("true != false"));
    assert!(boolean("(1 < 2) == (3 < 4)"));
    assert!(boolean("@hp.current < @hp.max / 4"));
}

#[test]
fn logic() {
    assert!(boolean("true && true"));
    assert!(!boolean("true && false"));
    assert!(boolean("false || true"));
    assert!(!boolean("false || false"));
    assert!(boolean("!false"));
    assert!(boolean("!!true"));
    assert!(boolean("true || false && false"), "&& binds tighter");
    assert!(boolean("1 + 1 == 2 && 2 * 2 == 4"));
    assert!(!boolean("@dead"));
}

#[test]
fn short_circuit_and_lazy_if() {
    assert!(!boolean("false && 1 / 0 == 1"));
    assert!(boolean("true || @missing"));
    assert_eq!(int("if true then 1 else 1 / 0"), 1);
    assert_eq!(int("if false then @missing else 2"), 2);
    assert_eq!(
        eval("true && @missing"),
        Err(ExprError::UnknownReference {
            offset: 8,
            path: "missing".to_owned()
        })
    );
}

#[test]
fn if_then_else() {
    assert_eq!(int("if @str >= 15 then 2 else 0"), 2);
    assert_eq!(int("1 + (if false then 1 else 10)"), 11);
    assert_eq!(int("1 + if false then 1 else 10 * 2"), 21, "else is greedy");
    assert_eq!(
        int("if @hp.current <= 0 then 0 else if @hp.current < @hp.max / 4 then 1 else 2"),
        1
    );
    assert_eq!(int("if if true then false else true then 1 else 2"), 2);
    assert!(boolean("if true then true else false"));
}

#[test]
fn functions() {
    assert_eq!(int("min(3, 1, 2)"), 1);
    assert_eq!(int("max(3, 1, 2)"), 3);
    assert_eq!(int("min(5)"), 5);
    assert_eq!(eval("max(1 / 2, 1 / 3)").unwrap(), frac(1, 2));
    assert_eq!(int("floor(7 / 2)"), 3);
    assert_eq!(int("floor(-7 / 2)"), -4);
    assert_eq!(int("ceil(7 / 2)"), 4);
    assert_eq!(int("ceil(-7 / 2)"), -3);
    assert_eq!(int("abs(-3)"), 3);
    assert_eq!(eval("abs(-1 / 2)").unwrap(), frac(1, 2));
    assert_eq!(int("floor(5)"), 5);
    assert_eq!(int("max( 1 ,2 )"), 2);
    assert_eq!(int("min(max(1, 2), 3)"), 2);
}

#[test]
fn round_halves_away_from_zero() {
    for (input, expected) in [
        ("round(1 / 2)", 1),
        ("round(-1 / 2)", -1),
        ("round(5 / 2)", 3),
        ("round(-5 / 2)", -3),
        ("round(7 / 2)", 4),
        ("round(-7 / 2)", -4),
        ("round(4 / 3)", 1),
        ("round(5 / 3)", 2),
        ("round(-4 / 3)", -1),
        ("round(-5 / 3)", -2),
        ("round(0)", 0),
        ("round(1 / 3)", 0),
        ("round(-1 / 3)", 0),
    ] {
        assert_eq!(int(input), expected, "{input}");
    }
}

#[test]
fn ability_modifier_from_1_to_30() {
    let expr = Expr::parse("floor((@str - 10) / 2)").unwrap();
    let expected = [
        -5, -4, -4, -3, -3, -2, -2, -1, -1, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8,
        9, 9, 10,
    ];
    for (score, want) in (1..=30).zip(expected) {
        let sheet = BTreeMap::from([("str".to_owned(), score)]);
        assert_eq!(expr.eval_integer(&sheet).unwrap(), want, "score {score}");
    }
}

#[test]
fn type_errors() {
    let mismatch = |offset, expected, found| ExprError::TypeMismatch {
        offset,
        expected,
        found,
    };
    use ValueType::{Bool, Number};
    assert_eq!(eval("true + 1"), Err(mismatch(5, Number, Bool)));
    assert_eq!(eval("1 < false"), Err(mismatch(2, Number, Bool)));
    assert_eq!(eval("-true"), Err(mismatch(0, Number, Bool)));
    assert_eq!(eval("!1"), Err(mismatch(0, Bool, Number)));
    assert_eq!(eval("1 && true"), Err(mismatch(2, Bool, Number)));
    assert_eq!(eval("true || 1 == 1 || 2"), Ok(Value::Bool(true)));
    assert_eq!(eval("false || 2"), Err(mismatch(6, Bool, Number)));
    assert_eq!(eval("if 1 then 2 else 3"), Err(mismatch(3, Bool, Number)));
    assert_eq!(eval("1 == true"), Err(mismatch(2, Number, Bool)));
    assert_eq!(eval("min(1, true)"), Err(mismatch(7, Number, Bool)));
    assert_eq!(eval("floor(@dead)"), Err(mismatch(6, Number, Bool)));
}

#[test]
fn arithmetic_errors() {
    assert_eq!(eval("1 / 0"), Err(ExprError::DivisionByZero { offset: 2 }));
    assert_eq!(
        eval("1 / (2 - 2)"),
        Err(ExprError::DivisionByZero { offset: 2 })
    );
    let max = i64::MAX;
    assert_eq!(
        eval(&format!("{max} + 1")),
        Err(ExprError::Overflow { offset: 20 })
    );
    assert_eq!(
        eval(&format!("{max} * {max}")),
        Err(ExprError::Overflow { offset: 20 })
    );
    assert_eq!(
        eval(&format!("-{max} - 1")),
        Ok(Value::from(i64::MIN)),
        "i64::MIN is reachable"
    );
    assert_eq!(
        eval(&format!("-(-{max} - 1)")),
        Err(ExprError::Overflow { offset: 0 })
    );
    assert_eq!(
        eval(&format!("abs(-{max} - 1)")),
        Err(ExprError::Overflow { offset: 0 })
    );
    assert_eq!(
        eval(&format!("1 / {max} / {max}")),
        Err(ExprError::Overflow { offset: 24 })
    );
    assert_eq!(
        Expr::parse("9223372036854775808"),
        Err(ExprError::IntegerOutOfRange { offset: 0 })
    );
}

#[test]
fn as_integer() {
    assert_eq!(Value::from(3).as_integer(), Ok(3));
    let err = eval("7 / 2").unwrap().as_integer().unwrap_err();
    assert_eq!(err.code(), "not_an_integer");
    assert_eq!(err.to_string(), "7/2 is not an integer");
    assert_eq!(
        Value::from(true).as_integer().unwrap_err().code(),
        "not_an_integer"
    );
    assert_eq!(
        Expr::parse("7 / 2").unwrap().eval_integer(&NoContext),
        Err(ExprError::NotAnInteger { value: frac(7, 2) })
    );
}

#[test]
fn references() {
    let expr = Expr::parse("@b + @a.x * @b - min(@c, if @d then 1 else @a.x)").unwrap();
    assert_eq!(
        expr.references().into_iter().collect::<Vec<_>>(),
        ["a.x", "b", "c", "d"]
    );
    assert!(Expr::parse("1 + 2").unwrap().references().is_empty());
}

struct Failing;

impl Context for Failing {
    fn lookup(&self, path: &str) -> Result<Value, LookupError> {
        Err(LookupError::Other(format!("cannot read {path}")))
    }
}

#[test]
fn lookup_errors() {
    assert_eq!(
        Expr::parse("1 + @x").unwrap().eval(&NoContext),
        Err(ExprError::UnknownReference {
            offset: 4,
            path: "x".to_owned()
        })
    );
    let err = Expr::parse("@x.y").unwrap().eval(&Failing).unwrap_err();
    assert_eq!(err.code(), "reference");
    assert_eq!(err.to_string(), "field @x.y at position 0: cannot read x.y");
}

#[test]
fn contexts() {
    let ints: BTreeMap<String, i64> = BTreeMap::from([("a".to_owned(), 2)]);
    let hash: std::collections::HashMap<String, i64> = ints.clone().into_iter().collect();
    let expr = Expr::parse("@a * 3").unwrap();
    assert_eq!(expr.eval_integer(&ints), Ok(6));
    assert_eq!(expr.eval_integer(&hash), Ok(6));
    assert_eq!(expr.eval_integer(&&ints), Ok(6));
    let dynamic: &dyn Context = &ints;
    assert_eq!(expr.eval_integer(dynamic), Ok(6));
}

#[test]
fn limits() {
    let long = format!("1{}", " ".repeat(MAX_EXPR_LEN));
    assert_eq!(
        Expr::parse(&long),
        Err(ExprError::TooLong {
            len: MAX_EXPR_LEN + 1
        })
    );
    let at_limit = format!("1{}", " ".repeat(MAX_EXPR_LEN - 1));
    assert!(Expr::parse(&at_limit).is_ok());
    // Characters, not bytes.
    assert_eq!(
        Expr::parse(&"ж".repeat(MAX_EXPR_LEN + 1)),
        Err(ExprError::TooLong {
            len: MAX_EXPR_LEN + 1
        })
    );
    assert_eq!(
        Expr::parse(&"ж".repeat(MAX_EXPR_LEN)).unwrap_err().code(),
        "syntax"
    );
    assert_eq!(int(&format!("{}1", "-".repeat(MAX_DEPTH))), 1);

    let nested = |n: usize| format!("{}1{}", "(".repeat(n), ")".repeat(n));
    assert_eq!(int(&nested(MAX_DEPTH)), 1);
    assert_eq!(
        Expr::parse(&nested(MAX_DEPTH + 1)),
        Err(ExprError::TooDeep { offset: MAX_DEPTH })
    );
    assert!(matches!(
        Expr::parse(&format!("{}1", "-".repeat(MAX_DEPTH + 1))),
        Err(ExprError::TooDeep { .. })
    ));
    let calls = format!("{}1{}", "abs(".repeat(MAX_DEPTH), ")".repeat(MAX_DEPTH));
    assert_eq!(int(&calls), 1);
    let calls = format!(
        "{}1{}",
        "abs(".repeat(MAX_DEPTH + 1),
        ")".repeat(MAX_DEPTH + 1)
    );
    assert!(matches!(
        Expr::parse(&calls),
        Err(ExprError::TooDeep { .. })
    ));
    // Long binary chains are not nesting.
    let chain = vec!["1"; 500].join("+");
    assert_eq!(int(&chain), 500);
    assert_eq!(Expr::parse("  "), Err(ExprError::Empty));
    assert_eq!(Expr::parse(""), Err(ExprError::Empty));
}

#[test]
fn canonical_display() {
    for (input, canonical) in [
        ("1+2*3", "1 + 2 * 3"),
        ("(1+2)*3", "(1 + 2) * 3"),
        ("1-(2-3)", "1 - (2 - 3)"),
        ("(1-2)-3", "1 - 2 - 3"),
        ("-(1)", "-1"),
        ("- -1", "--1"),
        ("-(1*2)", "-(1 * 2)"),
        ("!(1<2)", "!(1 < 2)"),
        ("(1<2)==(3<4)", "(1 < 2) == (3 < 4)"),
        ("min()", ""),
        ("min( 1,2 )", "min(1, 2)"),
        ("if(@a)then(1)else(2)", "if @a then 1 else 2"),
        ("1+if true then 2 else 3", "1 + (if true then 2 else 3)"),
        ("(if true then 2 else 3)+1", "(if true then 2 else 3) + 1"),
        ("true||false&&true", "true || false && true"),
        ("(true||false)&&true", "(true || false) && true"),
        ("floor((@str-10)/2)", "floor((@str - 10) / 2)"),
    ] {
        if canonical.is_empty() {
            assert!(Expr::parse(input).is_err());
            continue;
        }
        let expr = Expr::parse(input).unwrap();
        assert_eq!(expr.to_string(), canonical, "{input}");
        assert_eq!(Expr::parse(canonical).unwrap(), expr, "{input}");
        assert_eq!(expr.source(), input);
    }
}

#[test]
fn serde_keeps_source() {
    let expr = Expr::parse("floor( (@str-10)/2 )").unwrap();
    let json = serde_json::to_string(&expr).unwrap();
    assert_eq!(json, r#""floor( (@str-10)/2 )""#);
    let back: Expr = serde_json::from_str(&json).unwrap();
    assert_eq!(back, expr);
    assert_eq!(back.source(), expr.source());
    let err = serde_json::from_str::<Expr>(r#""1 +""#).unwrap_err();
    assert!(
        err.to_string().contains("syntax error at position 3"),
        "{err}"
    );
    assert_eq!(
        "2*3".parse::<Expr>().unwrap().eval_integer(&NoContext),
        Ok(6)
    );
}

fn defs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[test]
fn computed_fields_in_dependency_order() {
    let sheet = BTreeMap::from([("str".to_owned(), 16_i64), ("level".to_owned(), 5)]);
    let computed = Computed::resolve(
        &defs(&[
            ("attack", "@str_mod + @proficiency"),
            ("proficiency", "2 + floor((@level - 1) / 4)"),
            ("str_mod", "floor((@str - 10) / 2)"),
            ("level", "7"),
            ("half.level", "round(@level / 2)"),
        ]),
        &sheet,
    )
    .unwrap();
    assert_eq!(computed.get("str_mod"), Some(3));
    assert_eq!(computed.get("proficiency"), Some(3));
    assert_eq!(computed.get("attack"), Some(6));
    assert_eq!(computed.get("missing"), None);
    // A computed field shadows the sheet field for later formulas and the scope.
    assert_eq!(computed.get("level"), Some(7));
    assert_eq!(computed.get("half.level"), Some(4));
    let scope = computed.with_sheet(&sheet);
    assert_eq!(
        Expr::parse("@attack + @str + @level")
            .unwrap()
            .eval_integer(&scope),
        Ok(6 + 16 + 7)
    );
    assert_eq!(computed.values().len(), 5);
    assert_eq!(computed.clone().into_values().get("attack"), Some(&6));
    assert_eq!(
        Computed::resolve(&BTreeMap::new(), &NoContext).unwrap(),
        Computed::default()
    );
}

#[test]
fn computed_errors() {
    let sheet = BTreeMap::from([("str".to_owned(), 16_i64)]);
    let resolve = |pairs: &[(&str, &str)]| Computed::resolve(&defs(pairs), &sheet);

    assert_eq!(
        resolve(&[("a", "@b + 1"), ("b", "@c"), ("c", "@a * 2"), ("d", "1")]),
        Err(ExprError::Cycle {
            fields: vec!["a".into(), "b".into(), "c".into(), "a".into()]
        })
    );
    assert_eq!(
        resolve(&[("a", "@a")]),
        Err(ExprError::Cycle {
            fields: vec!["a".into(), "a".into()]
        })
    );
    // Unknown references fail even in an untaken branch.
    let err = resolve(&[("a", "if true then 1 else @nope")]).unwrap_err();
    assert_eq!(
        err,
        ExprError::Computed {
            field: "a".into(),
            source: Box::new(ExprError::UnknownReference {
                offset: 20,
                path: "nope".into()
            })
        }
    );
    assert_eq!(err.code(), "unknown_reference");
    assert_eq!(err.offset(), Some(20));
    assert_eq!(
        err.to_string(),
        "computed field `a`: unknown field @nope at position 20"
    );

    let err = resolve(&[("a", "@str / 3")]).unwrap_err();
    assert_eq!(err.code(), "not_an_integer");
    assert_eq!(
        err.to_string(),
        "computed field `a`: 16/3 is not an integer"
    );
    assert_eq!(
        resolve(&[("a", "@str > 3")]).unwrap_err().code(),
        "not_an_integer"
    );
    assert_eq!(resolve(&[("a", "1 +")]).unwrap_err().code(), "syntax");
    assert_eq!(
        resolve(&[("a", "1 / 0")]).unwrap_err().code(),
        "division_by_zero"
    );
    assert_eq!(
        resolve(&[("bad name", "1")]),
        Err(ExprError::InvalidFieldName {
            name: "bad name".into()
        })
    );
    for bad in ["", "1a", "a.", ".a", "@a", "a..b"] {
        assert_eq!(
            resolve(&[(bad, "1")]).unwrap_err().code(),
            "invalid_field_name"
        );
    }
}
