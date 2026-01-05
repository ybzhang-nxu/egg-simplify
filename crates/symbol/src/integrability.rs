use std::collections::{BTreeMap, BTreeSet};

use num_rational::Rational64;
use num_traits::Zero;

use crate::error::SymbolError;
use crate::integrability_utils::{build_envs, collect_vars, dlog};
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
