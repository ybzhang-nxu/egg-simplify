use std::collections::{BTreeMap, BTreeSet};

use mpl_ir::Expr;
use num_rational::Rational64;
use num_traits::Zero;

use crate::calculus::deriv;
use crate::error::{EvalError, SymbolError};
use crate::eval::eval;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum SampleTable {
    #[default]
    Default,
}

impl SampleTable {
    pub fn as_str(self) -> &'static str {
        match self {
            SampleTable::Default => "default",
        }
    }
}

impl std::str::FromStr for SampleTable {
    type Err = ();

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "default" => Ok(SampleTable::Default),
            _ => Err(()),
        }
    }
}

pub(crate) fn collect_vars(expr: &Expr, vars: &mut BTreeSet<String>) {
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

pub(crate) fn build_envs(vars: &[String]) -> Vec<BTreeMap<String, Rational64>> {
    build_envs_with_table(vars, SampleTable::Default)
}

pub(crate) fn build_envs_with_table(
    vars: &[String],
    table: SampleTable,
) -> Vec<BTreeMap<String, Rational64>> {
    let values = [
        Rational64::new(2, 7),
        Rational64::new(3, 7),
        Rational64::new(2, 5),
        Rational64::new(3, 5),
        Rational64::new(4, 9),
        Rational64::new(5, 11),
    ];
    let mut envs = Vec::new();
    let env_count = match table {
        SampleTable::Default => 5,
    };
    for k in 0..env_count {
        let mut env = BTreeMap::new();
        for (j, var) in vars.iter().enumerate() {
            let value = values[(k + j) % values.len()];
            env.insert(var.clone(), value);
        }
        envs.push(env);
    }
    envs
}

pub(crate) fn dlog(
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

pub(crate) struct DlogCache {
    values: Vec<Vec<Vec<Option<Rational64>>>>,
}

impl DlogCache {
    pub(crate) fn new(
        letters: &[Expr],
        vars: &[String],
        envs: &[BTreeMap<String, Rational64>],
    ) -> Result<Self, SymbolError> {
        let mut values = vec![vec![vec![None; vars.len()]; letters.len()]; envs.len()];
        for (env_idx, env) in envs.iter().enumerate() {
            for (letter_idx, letter) in letters.iter().enumerate() {
                for (var_idx, var) in vars.iter().enumerate() {
                    values[env_idx][letter_idx][var_idx] = dlog(letter, var, env)?;
                }
            }
        }
        Ok(Self { values })
    }

    pub(crate) fn get(
        &self,
        env_idx: usize,
        letter_idx: usize,
        var_idx: usize,
    ) -> Option<Rational64> {
        self.values[env_idx][letter_idx][var_idx]
    }
}
