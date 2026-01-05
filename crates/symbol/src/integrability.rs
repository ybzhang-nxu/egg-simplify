use std::collections::{BTreeMap, BTreeSet};

use mpl_ir::Expr;
use num_rational::Rational64;
use num_traits::Zero;

use crate::calculus::deriv;
use crate::error::{EvalError, SymbolError};
use crate::eval::eval;
use crate::tensor::Symbol;

pub fn check_integrable(sym: &Symbol) -> Result<bool, SymbolError> {
    let mut has_weight2 = false;
    for (word, coeff) in sym.terms() {
        let weight = word.letters().len();
        if weight > 2 {
            return Err(SymbolError::NotImplemented(
                "integrability for weight > 2".to_string(),
            ));
        }
        if weight == 2 && !coeff.is_zero() {
            has_weight2 = true;
        }
    }

    if !has_weight2 {
        return Ok(true);
    }

    let mut vars = BTreeSet::new();
    for (word, coeff) in sym.terms() {
        if word.letters().len() == 2 && !coeff.is_zero() {
            for letter in word.letters() {
                collect_vars(letter, &mut vars);
            }
        }
    }

    if vars.len() < 2 {
        return Ok(true);
    }

    let vars: Vec<String> = vars.into_iter().collect();
    let envs = build_envs(&vars);

    for i in 0..vars.len() {
        for j in (i + 1)..vars.len() {
            let vi = &vars[i];
            let vj = &vars[j];
            let mut valid = 0;
            for env in &envs {
                if let Some(value) = wedge_value(sym, vi, vj, env)? {
                    valid += 1;
                    if !value.is_zero() {
                        return Ok(false);
                    }
                }
            }
            if valid < 2 {
                return Err(SymbolError::InsufficientSamples);
            }
        }
    }

    Ok(true)
}

fn collect_vars(expr: &Expr, vars: &mut BTreeSet<String>) {
    match expr {
        Expr::Var(name) => {
            vars.insert(name.clone());
        }
        Expr::Add(children) | Expr::Mul(children) => {
            for child in children {
                collect_vars(child, vars);
            }
        }
        Expr::Neg(inner) => collect_vars(inner, vars),
        Expr::Pow(base, _) => collect_vars(base, vars),
        Expr::Rational(_) => {}
        Expr::Log(_) | Expr::Li2(_) => {}
    }
}

fn build_envs(vars: &[String]) -> Vec<BTreeMap<String, Rational64>> {
    let values = [
        Rational64::new(2, 7),
        Rational64::new(3, 7),
        Rational64::new(2, 5),
        Rational64::new(3, 5),
        Rational64::new(4, 9),
        Rational64::new(5, 11),
    ];
    let mut envs = Vec::new();
    for k in 0..5 {
        let mut env = BTreeMap::new();
        for (j, var) in vars.iter().enumerate() {
            let value = values[(k + j) % values.len()];
            env.insert(var.clone(), value);
        }
        envs.push(env);
    }
    envs
}

fn wedge_value(
    sym: &Symbol,
    vi: &str,
    vj: &str,
    env: &BTreeMap<String, Rational64>,
) -> Result<Option<Rational64>, SymbolError> {
    let mut total = Rational64::zero();
    for (word, coeff) in sym.terms() {
        if word.letters().len() != 2 || coeff.is_zero() {
            continue;
        }
        let l1 = &word.letters()[0];
        let l2 = &word.letters()[1];
        let dlog_l1_vi = match dlog(l1, vi, env)? {
            Some(value) => value,
            None => return Ok(None),
        };
        let dlog_l2_vj = match dlog(l2, vj, env)? {
            Some(value) => value,
            None => return Ok(None),
        };
        let dlog_l1_vj = match dlog(l1, vj, env)? {
            Some(value) => value,
            None => return Ok(None),
        };
        let dlog_l2_vi = match dlog(l2, vi, env)? {
            Some(value) => value,
            None => return Ok(None),
        };

        let term = dlog_l1_vi * dlog_l2_vj - dlog_l1_vj * dlog_l2_vi;
        total += *coeff * term;
    }
    Ok(Some(total))
}

fn dlog(
    letter: &Expr,
    var: &str,
    env: &BTreeMap<String, Rational64>,
) -> Result<Option<Rational64>, SymbolError> {
    let denom = match eval(letter, env) {
        Ok(value) => value,
        Err(EvalError::NegativePowerOfZero) => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    if denom.is_zero() {
        return Ok(None);
    }
    let deriv = deriv(letter, var)?;
    let numer = match eval(&deriv, env) {
        Ok(value) => value,
        Err(EvalError::NegativePowerOfZero) => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    Ok(Some(numer / denom))
}
