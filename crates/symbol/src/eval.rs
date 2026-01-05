use std::collections::BTreeMap;

use mpl_ir::Expr;
use num_rational::Rational64;
use num_traits::{One, Zero};

use crate::error::EvalError;

pub(crate) fn eval(
    expr: &Expr,
    env: &BTreeMap<String, Rational64>,
) -> Result<Rational64, EvalError> {
    match expr {
        Expr::Rational(value) => Ok(*value),
        Expr::Var(name) => env
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::UnknownVariable(name.clone())),
        Expr::Add(children) => {
            let mut sum = Rational64::zero();
            for child in children {
                sum += eval(child, env)?;
            }
            Ok(sum)
        }
        Expr::Mul(children) => {
            let mut product = Rational64::one();
            for child in children {
                product *= eval(child, env)?;
            }
            Ok(product)
        }
        Expr::Neg(inner) => Ok(-eval(inner, env)?),
        Expr::Pow(base, exp) => {
            let value = eval(base, env)?;
            eval_pow(value, *exp)
        }
        Expr::Log(_) | Expr::Li2(_) => Err(EvalError::UnknownVariable("log/li2".to_string())),
    }
}

fn eval_pow(value: Rational64, exp: i32) -> Result<Rational64, EvalError> {
    if exp == 0 {
        return Ok(Rational64::one());
    }
    if exp == i32::MIN {
        return Err(EvalError::PowerOverflow);
    }
    let numer = *value.numer();
    let denom = *value.denom();
    if exp < 0 && numer == 0 {
        return Err(EvalError::NegativePowerOfZero);
    }
    let exp_abs = exp.unsigned_abs();
    let (base_numer, base_denom) = if exp >= 0 {
        (numer, denom)
    } else {
        (denom, numer)
    };
    let numer_pow = base_numer
        .checked_pow(exp_abs)
        .ok_or(EvalError::PowerOverflow)?;
    let denom_pow = base_denom
        .checked_pow(exp_abs)
        .ok_or(EvalError::PowerOverflow)?;
    Ok(Rational64::new(numer_pow, denom_pow))
}
