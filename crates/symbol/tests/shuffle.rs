use mpl_ir::{parse_sexpr, Expr};
use mpl_symbol::space::check_integrable_n;
use mpl_symbol::{symbol, Symbol, Word};
use num_rational::Rational64;

fn r(numer: i64, denom: i64) -> Rational64 {
    Rational64::new(numer, denom)
}

fn normalized(input: &str) -> Expr {
    parse_sexpr(input).expect("parse").normalize()
}

#[test]
fn symbol_log_pow_three_is_shuffle_factorial() {
    let expr = normalized("(^ (log x) 3)");
    let sym = symbol(&expr).expect("symbol");
    let x = normalized("x");
    let expected = Symbol::from_terms(vec![(Word(vec![x.clone(), x.clone(), x]), r(6, 1))]);
    assert_eq!(sym, expected);
}

#[test]
fn symbol_log_log_is_shuffle_sum() {
    let expr = normalized("(* (log a) (log b))");
    let sym = symbol(&expr).expect("symbol");
    let a = normalized("a");
    let b = normalized("b");
    let expected = Symbol::from_terms(vec![
        (Word(vec![a.clone(), b.clone()]), r(1, 1)),
        (Word(vec![b, a]), r(1, 1)),
    ]);
    assert_eq!(sym, expected);
}

#[test]
fn symbol_li2_log_shuffle_weight3() {
    let expr = normalized("(* (li2 x) (log y))");
    let sym = symbol(&expr).expect("symbol");
    let one_minus_x = normalized("(+ 1 (* -1 x))");
    let x = normalized("x");
    let y = normalized("y");
    let expected = Symbol::from_terms(vec![
        (
            Word(vec![one_minus_x.clone(), x.clone(), y.clone()]),
            r(-1, 1),
        ),
        (
            Word(vec![one_minus_x.clone(), y.clone(), x.clone()]),
            r(-1, 1),
        ),
        (Word(vec![y, one_minus_x, x]), r(-1, 1)),
    ]);
    assert_eq!(sym, expected);
}

#[test]
fn symbol_li2_li2_shuffle_weight4() {
    let expr = normalized("(* (li2 x) (li2 y))");
    let sym = symbol(&expr).expect("symbol");
    let a = normalized("(+ 1 (* -1 x))");
    let b = normalized("x");
    let c = normalized("(+ 1 (* -1 y))");
    let d = normalized("y");
    let expected = Symbol::from_terms(vec![
        (
            Word(vec![a.clone(), b.clone(), c.clone(), d.clone()]),
            r(1, 1),
        ),
        (
            Word(vec![a.clone(), c.clone(), b.clone(), d.clone()]),
            r(1, 1),
        ),
        (
            Word(vec![a.clone(), c.clone(), d.clone(), b.clone()]),
            r(1, 1),
        ),
        (
            Word(vec![c.clone(), a.clone(), b.clone(), d.clone()]),
            r(1, 1),
        ),
        (
            Word(vec![c.clone(), a.clone(), d.clone(), b.clone()]),
            r(1, 1),
        ),
        (Word(vec![c, d, a, b]), r(1, 1)),
    ]);
    assert_eq!(sym, expected);
}

#[test]
fn integrability_n_holds_for_shuffle_examples() {
    let exprs = [
        "(^ (log x) 3)",
        "(* (li2 x) (log y))",
        "(* (li2 x) (li2 y))",
    ];
    for expr in exprs {
        let sym = symbol(&normalized(expr)).expect("symbol");
        assert!(check_integrable_n(&sym).expect("integrable"));
    }
}

#[test]
fn symbol_to_string_is_deterministic() {
    let expr = normalized("(* (li2 x) (log y))");
    let first = symbol(&expr).expect("symbol").to_string();
    for _ in 0..10 {
        let next = symbol(&expr).expect("symbol").to_string();
        assert_eq!(first, next);
    }
}
