use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use mpl_ir::Expr;
use num_rational::Rational64;
use num_traits::{One, Zero};
use thiserror::Error;

pub type Coeff = Rational64;

#[derive(Clone, Debug)]
pub struct Word(pub Vec<Expr>);

impl Word {
    pub fn letters(&self) -> &[Expr] {
        &self.0
    }
}

impl PartialEq for Word {
    fn eq(&self, other: &Self) -> bool {
        word_key(self) == word_key(other)
    }
}

impl Eq for Word {}

impl PartialOrd for Word {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Word {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut left = self.0.iter().map(|expr| expr.to_canonical_string());
        let mut right = other.0.iter().map(|expr| expr.to_canonical_string());
        loop {
            match (left.next(), right.next()) {
                (Some(a), Some(b)) => {
                    let order = a.cmp(&b);
                    if order != Ordering::Equal {
                        return order;
                    }
                }
                (None, Some(_)) => return Ordering::Less,
                (Some(_), None) => return Ordering::Greater,
                (None, None) => return Ordering::Equal,
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    terms: BTreeMap<Word, Coeff>,
}

#[derive(Debug, Error)]
pub enum SymbolError {
    #[error("not implemented: {0}")]
    NotImplemented(String),
    #[error(transparent)]
    Eval(#[from] EvalError),
    #[error("insufficient valid sample points for integrability check")]
    InsufficientSamples,
}

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("unknown variable '{0}'")]
    UnknownVariable(String),
    #[error("negative exponent on zero")]
    NegativePowerOfZero,
    #[error("overflow while computing power")]
    PowerOverflow,
}

impl Symbol {
    pub fn zero() -> Self {
        Self {
            terms: BTreeMap::new(),
        }
    }

    pub fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }

    pub fn terms(&self) -> impl Iterator<Item = (&Word, &Coeff)> {
        self.terms.iter()
    }

    fn add_term(&mut self, word: Word, coeff: Coeff) {
        if coeff.is_zero() {
            return;
        }
        use std::collections::btree_map::Entry;
        match self.terms.entry(word) {
            Entry::Vacant(entry) => {
                entry.insert(coeff);
            }
            Entry::Occupied(mut entry) => {
                let updated = *entry.get() + coeff;
                if updated.is_zero() {
                    entry.remove();
                } else {
                    entry.insert(updated);
                }
            }
        }
    }

    fn add_assign(&mut self, other: Symbol) {
        for (word, coeff) in other.terms {
            self.add_term(word, coeff);
        }
    }

    fn scale(&mut self, coeff: Coeff) {
        if coeff.is_zero() {
            self.terms.clear();
            return;
        }
        for value in self.terms.values_mut() {
            *value *= coeff;
        }
    }
}

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

pub fn check_integrable(sym: &Symbol) -> Result<bool, SymbolError> {
    let mut has_weight2 = false;
    for (word, coeff) in sym.terms.iter() {
        let weight = word.0.len();
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
    for (word, coeff) in sym.terms.iter() {
        if word.0.len() == 2 && !coeff.is_zero() {
            for letter in &word.0 {
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
                match wedge_value(sym, vi, vj, env)? {
                    Some(value) => {
                        valid += 1;
                        if !value.is_zero() {
                            return Ok(false);
                        }
                    }
                    None => {}
                }
            }
            if valid < 2 {
                return Err(SymbolError::InsufficientSamples);
            }
        }
    }

    Ok(true)
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
    out.add_term(
        Word(vec![one_minus, letter]),
        Rational64::from_integer(-1),
    );
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
            out.add_term(
                Word(vec![left_letter.clone(), right_letter.clone()]),
                coeff,
            );
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

fn word_key(word: &Word) -> Vec<String> {
    word.0
        .iter()
        .map(|expr| expr.to_canonical_string())
        .collect()
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
    for (word, coeff) in sym.terms.iter() {
        if word.0.len() != 2 || coeff.is_zero() {
            continue;
        }
        let l1 = &word.0[0];
        let l2 = &word.0[1];
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

fn deriv(expr: &Expr, var: &str) -> Result<Expr, SymbolError> {
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

fn eval(expr: &Expr, env: &BTreeMap<String, Rational64>) -> Result<Rational64, EvalError> {
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
        Expr::Log(_) | Expr::Li2(_) => Err(EvalError::UnknownVariable(
            "log/li2".to_string(),
        )),
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
    let (base_numer, base_denom) = if exp >= 0 { (numer, denom) } else { (denom, numer) };
    let numer_pow = base_numer
        .checked_pow(exp_abs)
        .ok_or(EvalError::PowerOverflow)?;
    let denom_pow = base_denom
        .checked_pow(exp_abs)
        .ok_or(EvalError::PowerOverflow)?;
    Ok(Rational64::new(numer_pow, denom_pow))
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

#[cfg(test)]
mod tests {
    use super::*;
    use mpl_ir::parse_sexpr;

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
        assert_eq!(check_integrable(&sym).expect("integrable"), true);
    }

    #[test]
    fn integrable_false_for_single_word() {
        let mut sym = Symbol::zero();
        sym.add_term(Word(vec![normalized("x"), normalized("y")]), r(1, 1));
        assert_eq!(check_integrable(&sym).expect("integrable"), false);
    }

    #[test]
    fn integrable_true_for_symmetric_word() {
        let mut sym = Symbol::zero();
        sym.add_term(Word(vec![normalized("x"), normalized("y")]), r(1, 1));
        sym.add_term(Word(vec![normalized("y"), normalized("x")]), r(1, 1));
        assert_eq!(check_integrable(&sym).expect("integrable"), true);
    }

    #[test]
    fn integrable_log_log() {
        let expr = normalized("(* (log x) (log y))");
        let sym = symbol(&expr).expect("symbol");
        assert_eq!(check_integrable(&sym).expect("integrable"), true);
    }
}
