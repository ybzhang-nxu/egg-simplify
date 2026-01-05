use mpl_ir::Expr;
use num_rational::Rational64;
use num_traits::{One, Zero};

use crate::error::SymbolError;
use crate::tensor::{Symbol, Word};

pub fn symbol(expr: &Expr) -> Result<Symbol, SymbolError> {
    match expr {
        Expr::Log(inner) => symbol_log(inner),
        Expr::Li2(inner) => symbol_li2(inner),
        Expr::Add(children) => {
            let mut out = Symbol::zero();
            for child in children {
                let child_symbol = symbol(child)?;
                out.add_assign(child_symbol);
            }
            Ok(out)
        }
        Expr::Mul(children) => symbol_mul(children),
        Expr::Neg(inner) => {
            let mut inner_symbol = symbol(inner)?;
            inner_symbol.scale(Rational64::from_integer(-1));
            Ok(inner_symbol)
        }
        Expr::Pow(base, exp) => symbol_pow(base, *exp),
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

fn symbol_mul(children: &[Expr]) -> Result<Symbol, SymbolError> {
    let mut coeff = Rational64::one();
    let mut factors = Vec::new();

    for child in children {
        match child {
            Expr::Rational(value) => {
                coeff *= *value;
            }
            Expr::Neg(inner) => {
                coeff *= Rational64::from_integer(-1);
                factors.push((**inner).clone());
            }
            other => factors.push(other.clone()),
        }
    }

    if coeff.is_zero() {
        return Ok(Symbol::zero());
    }

    if factors.is_empty() {
        return Ok(Symbol::zero());
    }

    if factors.len() == 1 {
        let mut inner = symbol(&factors[0])?;
        inner.scale(coeff);
        return Ok(inner);
    }

    if factors.len() == 2 {
        if let (Expr::Log(left), Expr::Log(right)) = (&factors[0], &factors[1]) {
            let left_letter = left.normalize();
            let right_letter = right.normalize();
            if !is_algebraic(&left_letter) || !is_algebraic(&right_letter) {
                return Err(SymbolError::NotImplemented(
                    "log letter must be algebraic".to_string(),
                ));
            }

            let mut out = Symbol::zero();
            out.add_term(Word(vec![left_letter.clone(), right_letter.clone()]), coeff);
            out.add_term(Word(vec![right_letter, left_letter]), coeff);
            return Ok(out);
        }
    }

    if factors.iter().all(is_algebraic) {
        Ok(Symbol::zero())
    } else {
        Err(SymbolError::NotImplemented(
            "symbol for product".to_string(),
        ))
    }
}

fn symbol_pow(base: &Expr, exp: i32) -> Result<Symbol, SymbolError> {
    match base {
        Expr::Log(inner) => {
            if exp == 2 {
                let letter = inner.normalize();
                if !is_algebraic(&letter) {
                    return Err(SymbolError::NotImplemented(
                        "log letter must be algebraic".to_string(),
                    ));
                }
                let mut out = Symbol::zero();
                out.add_term(
                    Word(vec![letter.clone(), letter]),
                    Rational64::from_integer(2),
                );
                Ok(out)
            } else {
                Err(SymbolError::NotImplemented(format!(
                    "symbol for (^ (log ...) {exp})"
                )))
            }
        }
        _ => {
            if is_algebraic(base) {
                Ok(Symbol::zero())
            } else {
                Err(SymbolError::NotImplemented(format!(
                    "symbol for (^ {} {exp})",
                    base.to_canonical_string()
                )))
            }
        }
    }
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
        parse_sexpr(input).expect("parse").normalize()
    }

    #[test]
    fn symbol_log_basic() {
        let expr = normalized("(log x)");
        let sym = symbol(&expr).expect("symbol");
        let mut expected = Symbol::zero();
        expected.add_term(Word(vec![normalized("x")]), r(1, 1));
        assert_eq!(sym, expected);
    }

    #[test]
    fn symbol_li2_basic() {
        let expr = normalized("(li2 x)");
        let sym = symbol(&expr).expect("symbol");
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
        let sym = symbol(&expr).expect("symbol");
        let mut expected = Symbol::zero();
        expected.add_term(Word(vec![normalized("x"), normalized("y")]), r(1, 1));
        expected.add_term(Word(vec![normalized("y"), normalized("x")]), r(1, 1));
        assert_eq!(sym, expected);
    }

    #[test]
    fn symbol_log_log_same() {
        let expr = normalized("(* (log x) (log x))");
        let sym = symbol(&expr).expect("symbol");
        let mut expected = Symbol::zero();
        expected.add_term(Word(vec![normalized("x"), normalized("x")]), r(2, 1));
        assert_eq!(sym, expected);
    }

    #[test]
    fn integrable_li2() {
        let expr = normalized("(li2 x)");
        let sym = symbol(&expr).expect("symbol");
        assert!(check_integrable(&sym).expect("integrable"));
    }

    #[test]
    fn integrable_false_for_single_word() {
        let mut sym = Symbol::zero();
        sym.add_term(Word(vec![normalized("x"), normalized("y")]), r(1, 1));
        assert!(!check_integrable(&sym).expect("integrable"));
    }

    #[test]
    fn integrable_true_for_symmetric_word() {
        let mut sym = Symbol::zero();
        sym.add_term(Word(vec![normalized("x"), normalized("y")]), r(1, 1));
        sym.add_term(Word(vec![normalized("y"), normalized("x")]), r(1, 1));
        assert!(check_integrable(&sym).expect("integrable"));
    }

    #[test]
    fn integrable_log_log() {
        let expr = normalized("(* (log x) (log y))");
        let sym = symbol(&expr).expect("symbol");
        assert!(check_integrable(&sym).expect("integrable"));
    }
}
