use mpl_ir::Expr;
use mpl_symbol::{apply_suffix_projection, Symbol, Word};
use num_rational::Rational64;

fn v(name: &str) -> Expr {
    Expr::Var(name.to_string()).normalize()
}

fn r(numer: i64, denom: i64) -> Rational64 {
    Rational64::new(numer, denom)
}

#[test]
fn suffix_projection_drops_suffix_terms() {
    let sym = Symbol::from_terms(vec![
        (Word(vec![v("a"), v("b"), v("c")]), r(2, 1)),
        (Word(vec![v("d"), v("b"), v("c")]), r(3, 1)),
        (Word(vec![v("b"), v("c")]), r(5, 1)),
        (Word(vec![v("a"), v("b")]), r(7, 1)),
    ]);

    let suffix = vec![v("b"), v("c")];
    let projected = apply_suffix_projection(&sym, &suffix);

    let expected = Symbol::from_terms(vec![
        (Word(vec![v("a")]), r(2, 1)),
        (Word(vec![v("d")]), r(3, 1)),
        (Word(Vec::new()), r(5, 1)),
    ]);

    assert_eq!(projected, expected);
}
