#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::collections::BTreeMap;

use dnd_dice::limits::{MAX_DICE, MAX_EXPR_LEN, MAX_SAFE_INTEGER, MAX_SIDES};
use dnd_dice::{DiceError, Expr, Sign, SplitMix64, Term, TermDetail, roll};
use proptest::prelude::*;

fn fields() -> BTreeMap<String, i64> {
    BTreeMap::from([
        ("a".to_owned(), 3),
        ("b.c".to_owned(), -7),
        ("big".to_owned(), MAX_SAFE_INTEGER),
        ("neg".to_owned(), -MAX_SAFE_INTEGER),
    ])
}

fn ws() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just(""), Just(" "), Just("  "), Just("\t")]
}

/// A syntactically and semantically valid term in some non-canonical spelling.
fn term() -> impl Strategy<Value = String> {
    let dice = (
        1u32..=12,
        prop_oneof![1u32..=20, Just(100), Just(1_000_000)],
        any::<bool>(),
        any::<bool>(),
        0u8..=6,
        any::<u32>(),
    )
        .prop_map(|(count, sides, percent, explicit_one, modifier, n)| {
            let d = if percent {
                "d%".to_owned()
            } else {
                format!("D{sides}")
            };
            let c = if count == 1 && !explicit_one {
                String::new()
            } else {
                format!("{count}")
            };
            let m = match modifier {
                1 => format!("kh{}", 1 + n % count),
                2 => format!(" KL{}", 1 + n % count),
                3 if count > 1 => format!("dh{}", 1 + n % (count - 1)),
                4 if count > 1 => format!("dl{}", 1 + n % (count - 1)),
                5 if count == 1 => "  adv".to_owned(),
                6 if count == 1 => " DIS".to_owned(),
                _ => String::new(),
            };
            format!("{c}{d}{m}")
        });
    prop_oneof![
        dice,
        (0u64..=1000).prop_map(|c| format!("{c}")),
        Just("0042".to_owned()),
        prop_oneof![Just("@a"), Just("@b.c")].prop_map(str::to_owned),
    ]
}

fn expression() -> impl Strategy<Value = String> {
    (
        prop_oneof![Just(""), Just("+"), Just("-"), Just("- ")],
        term(),
        prop::collection::vec(
            (ws(), prop_oneof![Just("+"), Just("-")], ws(), term()),
            0..6,
        ),
        ws(),
    )
        .prop_map(|(first_sign, first, rest, trailing)| {
            let mut s = format!("{first_sign}{first}");
            for (w1, op, w2, t) in rest {
                s.push_str(&format!("{w1}{op}{w2}{t}"));
            }
            s.push_str(trailing);
            s
        })
}

/// Theoretical bounds of a parsed expression with the given fields.
fn bounds(e: &Expr, f: &BTreeMap<String, i64>) -> (i64, i64) {
    let (mut lo, mut hi) = (0i64, 0i64);
    for t in e.terms() {
        let (tlo, thi) = match &t.term {
            Term::Constant(c) => (*c, *c),
            Term::Field(p) => (f[p], f[p]),
            Term::Dice(d) => {
                let k = i64::from(d.kept());
                (k, k * i64::from(d.sides.get()))
            }
        };
        match t.sign {
            Sign::Plus => (lo, hi) = (lo + tlo, hi + thi),
            Sign::Minus => (lo, hi) = (lo - thi, hi - tlo),
        }
    }
    (lo, hi)
}

proptest! {
    #[test]
    fn total_is_within_theoretical_bounds(s in expression(), seed in any::<u64>()) {
        let e = Expr::parse(&s).unwrap_or_else(|err| panic!("{s:?}: {err}"));
        let f = fields();
        match roll(&e, &f, &mut SplitMix64::new(seed)) {
            Ok(r) => {
                let (lo, hi) = bounds(&e, &f);
                prop_assert!((lo..=hi).contains(&r.total), "{s}: {} not in {lo}..={hi}", r.total);
                let mut sum = 0i64;
                for t in &r.terms {
                    if let TermDetail::Dice { dice, sides, .. } = &t.term {
                        prop_assert!(dice.iter().all(|d| (1..=*sides).contains(&d.value)));
                        let kept: i64 = dice.iter().filter(|d| d.kept).map(|d| i64::from(d.value)).sum();
                        prop_assert_eq!(kept, t.subtotal);
                    }
                    sum += if t.sign == Sign::Plus { t.subtotal } else { -t.subtotal };
                }
                prop_assert_eq!(sum, r.total);
            }
            Err(DiceError::TotalOutOfRange) => {
                let (lo, hi) = bounds(&e, &f);
                prop_assert!(lo < -MAX_SAFE_INTEGER || hi > MAX_SAFE_INTEGER);
            }
            Err(other) => prop_assert!(false, "{s}: {other}"),
        }
    }

    #[test]
    fn same_seed_same_roll(s in expression(), seed in any::<u64>()) {
        let e = Expr::parse(&s).unwrap();
        let f = fields();
        let a = roll(&e, &f, &mut SplitMix64::new(seed));
        let b = roll(&e, &f, &mut SplitMix64::new(seed));
        prop_assert_eq!(a, b);
    }

    #[test]
    fn display_round_trips_and_is_never_longer(s in expression()) {
        let e = Expr::parse(&s).unwrap();
        let canonical = e.to_string();
        prop_assert!(canonical.len() <= s.trim().len(), "{s:?} -> {canonical:?}");
        prop_assert_eq!(Expr::parse(&canonical).unwrap(), e);
    }

    #[test]
    fn arbitrary_input_never_bypasses_limits(
        s in "[0-9dD%+@. akhlvisADV_-]{0,300}",
    ) {
        check_limits(&s)?;
    }

    #[test]
    fn huge_numbers_never_bypass_limits(
        parts in prop::collection::vec((any::<u64>(), any::<u64>(), prop_oneof![Just(""), Just("kh"), Just("dl"), Just(" adv")], any::<u64>()), 1..8),
    ) {
        let s = parts
            .iter()
            .map(|(c, x, m, n)| match *m {
                "" => format!("{}d{}+{}", c % 2000, x % 2_000_000, n),
                " adv" => format!("{}d{}{m}", c % 3, x % 2_000_000),
                _ => format!("{}d{}{m}{}", c % 2000, x % 2_000_000, n % 2000),
            })
            .collect::<Vec<_>>()
            .join("+");
        check_limits(&s)?;
    }
}

fn check_limits(s: &str) -> Result<(), TestCaseError> {
    let Ok(e) = Expr::parse(s) else { return Ok(()) };
    prop_assert!(s.chars().count() <= MAX_EXPR_LEN);
    prop_assert!(e.dice_rolled() <= MAX_DICE);
    for t in e.terms() {
        match &t.term {
            Term::Dice(d) => {
                prop_assert!(d.count >= 1 && d.kept() >= 1 && d.kept() <= d.rolled());
                prop_assert!((1..=MAX_SIDES).contains(&d.sides.get()));
            }
            Term::Constant(c) => prop_assert!((0..=MAX_SAFE_INTEGER).contains(c)),
            Term::Field(_) => {}
        }
    }
    let canonical = e.to_string();
    prop_assert!(canonical.chars().count() <= MAX_EXPR_LEN);
    prop_assert_eq!(Expr::parse(&canonical).unwrap(), e.clone());
    let f = BTreeMap::from([("a".to_owned(), MAX_SAFE_INTEGER)]);
    if let Ok(r) = roll(&e, &f, &mut SplitMix64::new(0)) {
        prop_assert!(r.total.abs() <= MAX_SAFE_INTEGER);
    }
    Ok(())
}
