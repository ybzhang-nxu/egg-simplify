use mpl_ir::Expr;
use num_rational::Rational64;
use num_traits::{One, Zero};

use crate::error::SymbolError;
use crate::{ShuffleFuel, Symbol, Word};

pub fn symbol(expr: &Expr) -> Result<Symbol, SymbolError> {
    let mut fuel = ShuffleFuel::unlimited();
    symbol_with_fuel(expr, &mut fuel)
}

pub fn symbol_with_fuel(expr: &Expr, fuel: &mut ShuffleFuel) -> Result<Symbol, SymbolError> {
    symbol_inner(expr, fuel)
}

fn symbol_inner(expr: &Expr, fuel: &mut ShuffleFuel) -> Result<Symbol, SymbolError> {
    match expr {
        Expr::Log(inner) => symbol_log(inner),
        Expr::Li2(inner) => symbol_li2(inner),
        Expr::Add(children) => {
            let mut out = Symbol::zero();
            for child in children {
                let child_symbol = symbol_inner(child, fuel)?;
                out.add_assign(child_symbol);
            }
            Ok(out)
        }
        Expr::Mul(children) => symbol_mul(children, fuel),
        Expr::Neg(inner) => {
            let mut inner_symbol = symbol_inner(inner, fuel)?;
            inner_symbol.scale(Rational64::from_integer(-1));
            Ok(inner_symbol)
        }
        Expr::Pow(base, exp) => symbol_pow(base, *exp, fuel),
        _ => {
            if is_algebraic(expr) {
                Ok(Symbol::zero())
            } else {
                Err(SymbolError::NotImplemented(format!(
                    "symbol for {}",
                    expr.to_canonical_string()
                )))
            }
        }
    }
}

fn symbol_log(inner: &Expr) -> Result<Symbol, SymbolError> {
    let letter = inner.normalize();
    if !is_algebraic(&letter) {
        return Err(SymbolError::NotImplemented(
            "log letter must be algebraic".to_string(),
        ));
    }
    let mut out = Symbol::zero();
    out.add_term(Word(vec![letter]), Rational64::one());
    Ok(out)
}

fn symbol_li2(inner: &Expr) -> Result<Symbol, SymbolError> {
    let letter = inner.normalize();
    if !is_algebraic(&letter) {
        return Err(SymbolError::NotImplemented(
            "li2 letter must be algebraic".to_string(),
        ));
    }
    let one = Expr::Rational(Rational64::one());
    let minus_one = Expr::Rational(Rational64::from_integer(-1));
    let neg_letter = Expr::Mul(vec![minus_one, letter.clone()]);
    let one_minus = Expr::Add(vec![one, neg_letter]).normalize();

    let mut out = Symbol::zero();
    out.add_term(Word(vec![one_minus, letter]), Rational64::from_integer(-1));
    Ok(out)
}

fn symbol_mul(children: &[Expr], fuel: &mut ShuffleFuel) -> Result<Symbol, SymbolError> {
    let mut coeff = Rational64::one();
    let mut factors = Vec::new();
    let mut has_non_rational_prefactor = false;

    for child in children {
        match child {
            Expr::Rational(value) => {
                coeff *= *value;
            }
            Expr::Neg(inner) => {
                coeff *= Rational64::from_integer(-1);
                match inner.as_ref() {
                    Expr::Rational(value) => {
                        coeff *= *value;
                    }
                    other => {
                        if is_algebraic(other) {
                            has_non_rational_prefactor = true;
                        } else {
                            factors.push(other.clone());
                        }
                    }
                }
            }
            other => {
                if is_algebraic(other) {
                    has_non_rational_prefactor = true;
                } else {
                    factors.push(other.clone());
                }
            }
        }
    }

    if coeff.is_zero() {
        return Ok(Symbol::zero());
    }

    if factors.is_empty() {
        return Ok(Symbol::zero());
    }

    if has_non_rational_prefactor {
        return Err(SymbolError::NotImplemented(
            "symbol for non-rational prefactor".to_string(),
        ));
    }

    let mut iter = factors.into_iter();
    let first = match iter.next() {
        Some(factor) => factor,
        None => return Ok(Symbol::zero()),
    };
    let mut out = symbol_inner(&first, fuel)?;
    for factor in iter {
        let sym = symbol_inner(&factor, fuel)?;
        out = out.shuffle_mul(&sym, fuel)?;
    }
    out.scale(coeff);
    Ok(out)
}

fn symbol_pow(base: &Expr, exp: i32, fuel: &mut ShuffleFuel) -> Result<Symbol, SymbolError> {
    if exp == 0 {
        return Ok(Symbol::zero());
    }
    if exp < 0 {
        return Err(SymbolError::NotImplemented(format!(
            "symbol for (^ {} {exp})",
            base.to_canonical_string()
        )));
    }
    if is_algebraic(base) {
        return Ok(Symbol::zero());
    }
    let base_sym = symbol_inner(base, fuel)?;
    base_sym.shuffle_pow(exp as u32, fuel)
}

fn is_algebraic(expr: &Expr) -> bool {
    match expr {
        Expr::Rational(_) | Expr::Var(_) => true,
        Expr::Add(children) | Expr::Mul(children) => children.iter().all(is_algebraic),
        Expr::Neg(inner) => is_algebraic(inner),
        Expr::Pow(base, _) => is_algebraic(base),
        Expr::Log(_) | Expr::Li2(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::{check_integrable, symbol, Symbol, Word};
    use mpl_ir::parse_sexpr;
    use mpl_ir::Expr;
    use num_rational::Rational64;

    fn r(numer: i64, denom: i64) -> Rational64 {
        Rational64::new(numer, denom)
    }

    fn normalized(input: &str) -> Expr {
        parse_sexpr(input).unwrap().normalize()
    }

    #[test]
    fn symbol_log_basic() {
        let expr = normalized("(log x)");
        let sym = symbol(&expr).unwrap();
        let mut expected = Symbol::zero();
        expected.add_term(Word(vec![normalized("x")]), r(1, 1));
        assert_eq!(sym, expected);
    }

    #[test]
    fn symbol_li2_basic() {
        let expr = normalized("(li2 x)");
        let sym = symbol(&expr).unwrap();
        let mut expected = Symbol::zero();
        expected.add_term(
            Word(vec![normalized("(+ 1 (* -1 x))"), normalized("x")]),
            r(-1, 1),
        );
        assert_eq!(sym, expected);
    }

    #[test]
    fn symbol_log_log_basic() {
        let expr = normalized("(* (log x) (log y))");
        let sym = symbol(&expr).unwrap();
        let mut expected = Symbol::zero();
        expected.add_term(Word(vec![normalized("x"), normalized("y")]), r(1, 1));
        expected.add_term(Word(vec![normalized("y"), normalized("x")]), r(1, 1));
        assert_eq!(sym, expected);
    }

    #[test]
    fn symbol_log_log_same() {
        let expr = normalized("(* (log x) (log x))");
        let sym = symbol(&expr).unwrap();
        let mut expected = Symbol::zero();
        expected.add_term(Word(vec![normalized("x"), normalized("x")]), r(2, 1));
        assert_eq!(sym, expected);
    }

    #[test]
    fn integrable_li2() {
        let expr = normalized("(li2 x)");
        let sym = symbol(&expr).unwrap();
        assert!(check_integrable(&sym).unwrap());
    }

    #[test]
    fn integrable_false_for_single_word() {
        let mut sym = Symbol::zero();
        sym.add_term(Word(vec![normalized("x"), normalized("y")]), r(1, 1));
        assert!(!check_integrable(&sym).unwrap());
    }

    #[test]
    fn integrable_true_for_symmetric_word() {
        let mut sym = Symbol::zero();
        sym.add_term(Word(vec![normalized("x"), normalized("y")]), r(1, 1));
        sym.add_term(Word(vec![normalized("y"), normalized("x")]), r(1, 1));
        assert!(check_integrable(&sym).unwrap());
    }

    #[test]
    fn integrable_log_log() {
        let expr = normalized("(* (log x) (log y))");
        let sym = symbol(&expr).unwrap();
        assert!(check_integrable(&sym).unwrap());
    }
}
