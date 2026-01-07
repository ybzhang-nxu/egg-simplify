use mpl_ir::{parse_sexpr, Expr};
use mpl_symbol::space::check_integrable_n;
use mpl_symbol::{symbol_with_fuel, ShuffleFuel, SymbolError};

const FUEL_LOGS_8: u64 = 46_232;
const TERMS_LOGS_8: usize = 40_320;

fn normalized(input: &str) -> Expr {
    parse_sexpr(input).unwrap().normalize()
}

fn log_product_expr(names: &[&str]) -> Expr {
    let mut parts = Vec::with_capacity(names.len());
    for name in names {
        parts.push(format!("(log {name})"));
    }
    let expr = format!("(* {})", parts.join(" "));
    normalized(&expr)
}

#[test]
#[ignore]
fn stress_shuffle_full_expand_logs_8() {
    let expr = log_product_expr(&["a", "b", "c", "d", "e", "f", "g", "h"]);
    let mut fuel = ShuffleFuel::new(FUEL_LOGS_8);
    let sym = symbol_with_fuel(&expr, &mut fuel).unwrap();
    assert_eq!(sym.terms().count(), TERMS_LOGS_8);
    assert_eq!(fuel.remaining(), Some(0));
}

#[test]
#[ignore]
fn stress_shuffle_fuel_boundary_logs_8() {
    let expr = log_product_expr(&["a", "b", "c", "d", "e", "f", "g", "h"]);
    let mut fuel = ShuffleFuel::new(FUEL_LOGS_8 - 1);
    let result = symbol_with_fuel(&expr, &mut fuel);
    assert!(matches!(result, Err(SymbolError::FuelExhausted)));
}

#[test]
#[ignore]
fn stress_shuffle_abort_logs_20_small_fuel() {
    let expr = log_product_expr(&[
        "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r",
        "s", "t",
    ]);
    let mut fuel = ShuffleFuel::new(1_000);
    let result = symbol_with_fuel(&expr, &mut fuel);
    assert!(matches!(result, Err(SymbolError::FuelExhausted)));
}

#[test]
#[ignore]
fn stress_integrability_weight10_li2_logpow() {
    let expr = normalized("(* (li2 x) (^ (log y) 8))");
    let mut fuel = ShuffleFuel::new(1_900_000);
    let sym = symbol_with_fuel(&expr, &mut fuel).unwrap();
    assert!(check_integrable_n(&sym).unwrap());
}
