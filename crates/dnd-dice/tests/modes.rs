#![allow(clippy::unwrap_used)]
use dnd_dice::Expr;
#[test]
fn modes_combine_with_explicit_sources_without_stacking() {
    let e = Expr::parse("1d20 adv + 2").unwrap();
    assert_eq!(
        e.with_d20_mode(true, false).unwrap().to_string(),
        "d20 adv+2"
    );
    assert_eq!(e.with_d20_mode(false, true).unwrap().to_string(), "d20+2");
    assert_eq!(
        Expr::parse("d20+1d4")
            .unwrap()
            .with_d20_mode(true, true)
            .unwrap()
            .to_string(),
        "d20+d4"
    );
    assert!(
        Expr::parse("d6")
            .unwrap()
            .with_d20_mode(true, false)
            .is_err()
    );
}
