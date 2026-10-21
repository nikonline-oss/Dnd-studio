#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::collections::BTreeMap;

use dnd_dice::{Expr, SplitMix64, roll};

fn rolled(input: &str, seed: u64) -> dnd_dice::Roll {
    let fields = BTreeMap::from([("str_mod".to_owned(), 3)]);
    roll(
        &Expr::parse(input).unwrap(),
        &fields,
        &mut SplitMix64::new(seed),
    )
    .unwrap()
}

#[test]
fn attack_with_advantage() {
    insta::assert_yaml_snapshot!(rolled("1d20 adv + @str_mod + 2", 7));
}

#[test]
fn ability_score_4d6_keep_highest_3() {
    insta::assert_yaml_snapshot!(rolled("4d6kh3", 42));
}

#[test]
fn percentile_minus_drop_lowest() {
    insta::assert_yaml_snapshot!(rolled("d% - 3d4dl1", 2026));
}
