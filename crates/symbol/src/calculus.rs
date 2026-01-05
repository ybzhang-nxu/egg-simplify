use mpl_ir::Expr;
use num_rational::Rational64;
use num_traits::{One, Zero};

use crate::error::SymbolError;

pub(crate) fn deriv(expr: &Expr, var: &str) -> Result<Expr, SymbolError> {
    match expr {
        Expr::Rational(_) => Ok(Expr::Rational(Rational64::zero())),
        Expr::Var(name) => {
            if name == var {
                Ok(Expr::Rational(Rational64::one()))
            } else {
                Ok(Expr::Rational(Rational64::zero()))
            }
        }
        Expr::Add(children) => {
            let mut terms = Vec::new();
            for child in children {
                let derived = deriv(child, var)?;
                if !is_zero_expr(&derived) {
                    terms.push(derived);
                }
            }
            Ok(make_add(terms))
        }
        Expr::Mul(children) => {
            let mut terms = Vec::new();
            for (index, child) in children.iter().enumerate() {
                let derived = deriv(child, var)?;
                if is_zero_expr(&derived) {
                    continue;
                }
                let mut product = Vec::with_capacity(children.len());
                product.push(derived);
                for (j, other) in children.iter().enumerate() {
                    if index == j {
                        continue;
                    }
                    product.push(other.clone());
                }
                terms.push(make_mul(product));
            }
            Ok(make_add(terms))
        }
        Expr::Neg(inner) => Ok(Expr::Neg(Box::new(deriv(inner, var)?))),
        Expr::Pow(base, exp) => {
            if *exp == 0 {
                return Ok(Expr::Rational(Rational64::zero()));
            }
            let base_deriv = deriv(base, var)?;
            if is_zero_expr(&base_deriv) {
                return Ok(Expr::Rational(Rational64::zero()));
            }
            if *exp == 1 {
                return Ok(base_deriv);
            }
            let coeff = Expr::Rational(Rational64::from_integer(i64::from(*exp)));
            let power = Expr::Pow(Box::new((**base).clone()), exp - 1);
            Ok(make_mul(vec![coeff, power, base_deriv]))
        }
        Expr::Log(_) | Expr::Li2(_) => Err(SymbolError::NotImplemented(
            "derivative for log/li2 letters".to_string(),
        )),
    }
}

fn make_add(terms: Vec<Expr>) -> Expr {
    if terms.is_empty() {
        Expr::Rational(Rational64::zero())
    } else if terms.len() == 1 {
        terms.into_iter().next().unwrap()
    } else {
        Expr::Add(terms)
    }
}

fn make_mul(terms: Vec<Expr>) -> Expr {
    if terms.is_empty() {
        Expr::Rational(Rational64::one())
    } else if terms.len() == 1 {
        terms.into_iter().next().unwrap()
    } else {
        Expr::Mul(terms)
    }
}

fn is_zero_expr(expr: &Expr) -> bool {
    matches!(expr, Expr::Rational(value) if value.is_zero())
}
