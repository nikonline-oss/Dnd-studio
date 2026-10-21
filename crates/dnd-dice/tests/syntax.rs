#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::collections::BTreeMap;

use dnd_dice::limits::MAX_SAFE_INTEGER;
use dnd_dice::{
    Dice, DiceError, DiceRng, Expr, LookupError, Modifier, NoFields, Sides, Sign, SplitMix64, Term,
    TermDetail, roll,
};

fn canon(input: &str) -> String {
    Expr::parse(input)
        .unwrap_or_else(|e| panic!("{input:?}: {e}"))
        .to_string()
}

fn err(input: &str) -> DiceError {
    Expr::parse(input).expect_err(input)
}

fn single_dice(input: &str) -> Dice {
    match &Expr::parse(input).unwrap().terms()[0].term {
        Term::Dice(d) => *d,
        other => panic!("not dice: {other:?}"),
    }
}

#[test]
fn every_syntax_form_parses_to_its_canonical_form() {
    let cases = [
        ("1d20", "d20"),
        ("d20", "d20"),
        ("3d6", "3d6"),
        ("3D6", "3d6"),
        ("007d6", "7d6"),
        ("5", "5"),
        ("+5", "5"),
        ("-5", "-5"),
        ("- d4", "-d4"),
        ("d%", "d%"),
        ("2d%", "2d%"),
        ("d100", "d100"),
        ("4d6kh3", "4d6kh3"),
        ("4d6 KH3", "4d6kh3"),
        ("4d6kl1", "4d6kl1"),
        ("4d6dh1", "4d6dh1"),
        ("4d6dl1", "4d6dl1"),
        ("1d20 adv", "d20 adv"),
        ("d20   ADV", "d20 adv"),
        ("1d20 dis", "d20 dis"),
        ("@str_mod", "@str_mod"),
        ("@hp.max", "@hp.max"),
        ("@_a1.b_2.c", "@_a1.b_2.c"),
        ("1d20 + @str_mod", "d20+@str_mod"),
        ("  2d8 + 1d6 - 3 + @x  ", "2d8+d6-3+@x"),
        ("d20 adv + 5", "d20 adv+5"),
        ("\t1d4\n-\n1", "d4-1"),
    ];
    for (input, expected) in cases {
        assert_eq!(canon(input), expected, "input {input:?}");
    }
}

#[test]
fn structure_of_a_parsed_expression() {
    let e = Expr::parse("2d%kh1 - @a.b + 7").unwrap();
    let terms = e.terms();
    assert_eq!(terms.len(), 3);
    assert_eq!(terms[0].sign, Sign::Plus);
    assert_eq!(
        terms[0].term,
        Term::Dice(Dice {
            count: 2,
            sides: Sides::Percent,
            modifier: Some(Modifier::KeepHighest(1)),
        })
    );
    assert_eq!(terms[1].sign, Sign::Minus);
    assert_eq!(terms[1].term, Term::Field("a.b".into()));
    assert_eq!(terms[2].term, Term::Constant(7));
    assert_eq!(e.dice_rolled(), 2);

    let adv = single_dice("d20 adv");
    assert_eq!((adv.rolled(), adv.kept()), (2, 1));
    let dl = single_dice("4d6dl1");
    assert_eq!((dl.rolled(), dl.kept()), (4, 3));
}

#[test]
fn syntax_errors_report_an_offset() {
    let cases = [
        ("d", 1),
        ("1d", 2),
        ("1d20+", 5),
        ("1d20 +", 6),
        ("2d6x", 3),
        ("1d20adv", 4),
        ("4d6kh", 5),
        ("4d6k3", 3),
        ("@", 1),
        ("@1abc", 1),
        ("@a..b", 2),
        ("1d20 5", 5),
        ("d6d6", 2),
        ("1 + + 2", 4),
        ("--5", 1),
        ("1d20 advantage", 8),
        ("кубик", 0),
        ("(1d6)", 0),
        ("1d6*2", 3),
    ];
    for (input, offset) in cases {
        match err(input) {
            DiceError::Syntax {
                offset: got,
                message,
            } => {
                assert_eq!(got, offset, "{input:?}: {message}");
                assert!(!message.is_empty());
            }
            other => panic!("{input:?}: expected syntax error, got {other:?}"),
        }
    }
}

#[test]
fn limit_and_validation_errors() {
    assert_eq!(err(""), DiceError::Empty);
    assert_eq!(err("   "), DiceError::Empty);
    assert!(Expr::parse(&format!("{}11", "1+".repeat(127))).is_ok());
    assert!(matches!(
        err(&format!("{}1", "1+".repeat(128))),
        DiceError::TooLong { len: 257 }
    ));
    assert_eq!(err("0d6"), DiceError::ZeroDice);
    assert_eq!(err("1001d6"), DiceError::TooManyDice { count: 1001 });
    assert_eq!(
        err("99999999999999999999999d6"),
        DiceError::TooManyDice { count: u64::MAX }
    );
    assert_eq!(err("600d6+401d6"), DiceError::TooManyDice { count: 1001 });
    assert_eq!(err("999d6+d20 adv"), DiceError::TooManyDice { count: 1001 });
    assert_eq!(err("d0"), DiceError::InvalidSides { sides: 0 });
    assert_eq!(
        err("d1000001"),
        DiceError::InvalidSides { sides: 1_000_001 }
    );
    assert_eq!(
        err("4d6kh5"),
        DiceError::KeepDropOutOfRange {
            modifier: "kh",
            n: 5,
            count: 4
        }
    );
    assert_eq!(
        err("4d6kl0"),
        DiceError::KeepDropOutOfRange {
            modifier: "kl",
            n: 0,
            count: 4
        }
    );
    assert_eq!(
        err("4d6dl4"),
        DiceError::KeepDropOutOfRange {
            modifier: "dl",
            n: 4,
            count: 4
        }
    );
    assert_eq!(
        err("d20dh1"),
        DiceError::KeepDropOutOfRange {
            modifier: "dh",
            n: 1,
            count: 1
        }
    );
    assert_eq!(
        err("2d20 adv"),
        DiceError::AdvantageNeedsSingleDie { count: 2 }
    );
    assert_eq!(
        err("2d20 dis"),
        DiceError::AdvantageNeedsSingleDie { count: 2 }
    );
    assert_eq!(
        err("9007199254740992"),
        DiceError::ConstantOutOfRange {
            value: 9_007_199_254_740_992
        }
    );
    // Boundary values are fine.
    assert!(Expr::parse("1000d1000000").is_ok());
    assert!(Expr::parse("-9007199254740991").is_ok());
    assert!(Expr::parse("4d6kh4").is_ok());
    assert!(Expr::parse("4d6dl3").is_ok());
}

#[test]
fn errors_have_readable_messages() {
    assert_eq!(
        err("1001d6").to_string(),
        "too many dice: 1001 (max 1000 per expression)"
    );
    assert_eq!(
        err("4d6kh5").to_string(),
        "`kh5` is out of range for 4 dice"
    );
}

#[test]
fn rolls_constants_fields_and_signs() {
    let fields = BTreeMap::from([("str_mod".to_owned(), 3), ("hp.max".to_owned(), -10)]);
    let e = Expr::parse("5 + @str_mod - @hp.max - 2").unwrap();
    let r = roll(&e, &fields, &mut SplitMix64::new(1)).unwrap();
    assert_eq!(r.total, 5 + 3 + 10 - 2);
    assert_eq!(r.terms[2].subtotal, -10);
    assert_eq!(
        r.terms[2].term,
        TermDetail::Field {
            path: "hp.max".into(),
            value: -10
        }
    );
}

#[test]
fn field_errors() {
    let e = Expr::parse("d20 + @missing").unwrap();
    assert_eq!(
        roll(&e, &NoFields, &mut SplitMix64::new(1)),
        Err(DiceError::Field {
            path: "missing".into(),
            source: LookupError::Unknown
        })
    );
    let huge = BTreeMap::from([("x".to_owned(), MAX_SAFE_INTEGER + 1)]);
    assert_eq!(
        roll(&Expr::parse("@x").unwrap(), &huge, &mut SplitMix64::new(1)),
        Err(DiceError::FieldOutOfRange {
            path: "x".into(),
            value: MAX_SAFE_INTEGER + 1
        })
    );
    let big = BTreeMap::from([("x".to_owned(), MAX_SAFE_INTEGER)]);
    assert_eq!(
        roll(
            &Expr::parse("@x + 1").unwrap(),
            &big,
            &mut SplitMix64::new(1)
        ),
        Err(DiceError::TotalOutOfRange)
    );
    assert_eq!(
        roll(
            &Expr::parse("-@x - 1").unwrap(),
            &big,
            &mut SplitMix64::new(1)
        ),
        Err(DiceError::TotalOutOfRange)
    );
    assert_eq!(
        roll(&Expr::parse("@x").unwrap(), &big, &mut SplitMix64::new(1))
            .unwrap()
            .total,
        MAX_SAFE_INTEGER
    );
}

struct Counting(SplitMix64, u32);
impl DiceRng for Counting {
    fn next_u64(&mut self) -> u64 {
        self.1 += 1;
        self.0.next_u64()
    }
}

#[test]
fn lookup_errors_do_not_consume_randomness() {
    let mut rng = Counting(SplitMix64::new(1), 0);
    let e = Expr::parse("10d6 + @missing").unwrap();
    assert!(roll(&e, &NoFields, &mut rng).is_err());
    assert_eq!(rng.1, 0);
}

/// Replays fixed die faces for dice whose sides divide 300: the offset keeps every draw above the
/// rejection threshold (< sides) without changing it modulo sides.
struct Faces(Vec<u64>);
impl DiceRng for Faces {
    fn next_u64(&mut self) -> u64 {
        300 * (1 << 40) + self.0.remove(0) - 1
    }
}

fn kept(input: &str, faces: &[u64]) -> (Vec<(u32, bool)>, i64) {
    let r = roll(
        &Expr::parse(input).unwrap(),
        &NoFields,
        &mut Faces(faces.to_vec()),
    )
    .unwrap();
    let TermDetail::Dice { dice, .. } = &r.terms[0].term else {
        panic!("not dice")
    };
    (dice.iter().map(|d| (d.value, d.kept)).collect(), r.total)
}

#[test]
fn keep_drop_and_advantage_semantics() {
    assert_eq!(
        kept("4d6kh3", &[2, 5, 1, 5]),
        (vec![(2, true), (5, true), (1, false), (5, true)], 12)
    );
    assert_eq!(
        kept("4d6kl1", &[2, 5, 1, 5]),
        (vec![(2, false), (5, false), (1, true), (5, false)], 1)
    );
    // Ties: the earlier die is the one kept / dropped.
    assert_eq!(
        kept("3d6kh1", &[4, 4, 1]),
        (vec![(4, true), (4, false), (1, false)], 4)
    );
    assert_eq!(
        kept("3d6dh1", &[4, 4, 1]),
        (vec![(4, false), (4, true), (1, true)], 5)
    );
    assert_eq!(
        kept("4d6dl1", &[3, 3, 6, 2]),
        (vec![(3, true), (3, true), (6, true), (2, false)], 12)
    );
    assert_eq!(
        kept("d20 adv", &[7, 15]),
        (vec![(7, false), (15, true)], 15)
    );
    assert_eq!(kept("d20 dis", &[7, 15]), (vec![(7, true), (15, false)], 7));
    assert_eq!(kept("d%", &[100]), (vec![(100, true)], 100));
}

#[test]
fn expr_serializes_as_canonical_string_and_revalidates() {
    let e = Expr::parse("1d20 + 5").unwrap();
    assert_eq!(serde_json::to_string(&e).unwrap(), "\"d20+5\"");
    assert_eq!(serde_json::from_str::<Expr>("\"d20+5\"").unwrap(), e);
    assert!(serde_json::from_str::<Expr>("\"5000d6\"").is_err());
    let r = roll(&e, &NoFields, &mut SplitMix64::new(3)).unwrap();
    let json = serde_json::to_string(&r).unwrap();
    assert_eq!(serde_json::from_str::<dnd_dice::Roll>(&json).unwrap(), r);
}
