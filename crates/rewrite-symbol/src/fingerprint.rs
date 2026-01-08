use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mpl_ir::Expr;
use mpl_symbol::space::{Alphabet, Basis, WordConstraints};
use mpl_symbol::{ShuffleFuel, Symbol, SymbolError, Word};
use num_rational::Rational64;
use num_traits::Zero;

use crate::cache::{BasisKey, ExprKey, FingerprintCache, FingerprintKey};
use crate::error::{Fingerprint, RewriteSymbolError, UnknownReason, WeightFingerprint};
use crate::hash::StableHasher;

/// Configuration for expression fingerprinting.
#[derive(Clone, Debug)]
pub struct FingerprintConfig {
    /// Weight limit; weights above this return Unknown.
    pub weight_limit: Option<usize>,
    /// Deterministic budgeting configuration.
    pub budget: FingerprintBudget,
    /// Constraints for word enumeration.
    pub constraints: WordConstraints,
}

/// Budgeting configuration for fingerprinting.
#[derive(Clone, Debug)]
pub struct FingerprintBudget {
    /// Fuel consumed deterministically during fingerprinting.
    pub fuel: u64,
    /// Optional wall-clock limit in milliseconds.
    pub time_limit_ms: Option<u64>,
}

/// Compute a fingerprint for an expression.
pub fn fingerprint_expr(
    expr: &Expr,
    cfg: &FingerprintConfig,
    cache: &FingerprintCache,
) -> Result<Fingerprint, RewriteSymbolError> {
    let normalized = expr.normalize();
    let expr_key = cache.expr_key(&normalized);
    let cfg_hash = fingerprint_cfg_hash(cfg);
    let fp_key = FingerprintKey {
        expr: expr_key.clone(),
        cfg_hash,
    };

    let read_guard = match cache.expr_fp.read() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };
    if let Some(existing) = read_guard.get(&fp_key) {
        return Ok(existing.clone());
    }
    drop(read_guard);

    let fingerprint = fingerprint_expr_uncached(&normalized, &expr_key, cfg, cache)?;

    let mut write_guard = match cache.expr_fp.write() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };
    write_guard.insert(fp_key, fingerprint.clone());
    Ok(fingerprint)
}

fn fingerprint_expr_uncached(
    expr: &Expr,
    expr_key: &ExprKey,
    cfg: &FingerprintConfig,
    cache: &FingerprintCache,
) -> Result<Fingerprint, RewriteSymbolError> {
    if cfg.budget.fuel == 0 {
        return Ok(Fingerprint::Unknown {
            reason: UnknownReason::BudgetExhausted,
            expr_hash: expr_key.hash,
        });
    }

    let deadline = cfg
        .budget
        .time_limit_ms
        .map(|ms| Instant::now() + Duration::from_millis(ms));

    let mut symbol_fuel = ShuffleFuel::new(cfg.budget.fuel);
    let symbol = match symbol_cached(expr, expr_key, cache, &mut symbol_fuel) {
        Ok(symbol) => symbol,
        Err(err) => {
            return Ok(Fingerprint::Unknown {
                reason: map_symbol_error(err),
                expr_hash: expr_key.hash,
            })
        }
    };

    let mut terms_by_weight: BTreeMap<usize, Vec<(Word, Rational64)>> = BTreeMap::new();
    if let Some(constant) = extract_constant(expr) {
        if !constant.is_zero() {
            terms_by_weight
                .entry(0)
                .or_default()
                .push((Word(Vec::new()), constant));
        }
    }
    for (word, coeff) in symbol.terms() {
        if coeff.is_zero() {
            continue;
        }
        let weight = word.letters().len();
        terms_by_weight
            .entry(weight)
            .or_default()
            .push((word.clone(), *coeff));
    }

    if terms_by_weight.is_empty() {
        return Ok(Fingerprint::ByWeight(BTreeMap::new()));
    }

    let mut fuel = cfg.budget.fuel;
    let mut by_weight = BTreeMap::new();

    for (weight, terms) in terms_by_weight {
        if let Some(limit) = cfg.weight_limit {
            if weight > limit && weight != 0 {
                by_weight.insert(
                    weight,
                    WeightFingerprint::Unknown {
                        weight,
                        reason: UnknownReason::BudgetExhausted,
                        expr_hash: expr_key.hash,
                    },
                );
                continue;
            }
        }

        if fuel == 0 {
            return Ok(Fingerprint::Unknown {
                reason: UnknownReason::BudgetExhausted,
                expr_hash: expr_key.hash,
            });
        }
        fuel -= 1;

        if let Some(deadline) = deadline {
            if Instant::now() > deadline {
                return Ok(Fingerprint::Unknown {
                    reason: UnknownReason::BudgetExhausted,
                    expr_hash: expr_key.hash,
                });
            }
        }

        let wf = fingerprint_weight(weight, terms, expr_key.hash, cfg, cache);
        by_weight.insert(weight, wf);
    }

    Ok(Fingerprint::ByWeight(by_weight))
}

fn symbol_cached(
    expr: &Expr,
    expr_key: &ExprKey,
    cache: &FingerprintCache,
    fuel: &mut ShuffleFuel,
) -> Result<Symbol, SymbolError> {
    let read_guard = match cache.symbol.read() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };
    if let Some(symbol) = read_guard.get(expr_key) {
        return Ok(symbol.clone());
    }
    drop(read_guard);

    let symbol = mpl_symbol::symbol_with_fuel(expr, fuel)?;
    let mut write_guard = match cache.symbol.write() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };
    write_guard.insert(expr_key.clone(), symbol.clone());
    Ok(symbol)
}

fn extract_constant(expr: &Expr) -> Option<Rational64> {
    match expr {
        Expr::Rational(value) => Some(*value),
        Expr::Add(children) => children.iter().find_map(|child| match child {
            Expr::Rational(value) => Some(*value),
            _ => None,
        }),
        _ => None,
    }
}

fn fingerprint_weight(
    weight: usize,
    terms: Vec<(Word, Rational64)>,
    expr_hash: u64,
    cfg: &FingerprintConfig,
    cache: &FingerprintCache,
) -> WeightFingerprint {
    let (alphabet, alphabet_hash) = build_alphabet(&terms);
    let constraints_hash = constraints_hash(&cfg.constraints);
    let basis_id = basis_id(weight, alphabet_hash, constraints_hash);

    let basis_key = BasisKey {
        weight,
        alphabet_hash,
        constraints_hash,
    };

    let basis = match basis_cached(&basis_key, &alphabet, &cfg.constraints, cache) {
        Ok(basis) => basis,
        Err(reason) => {
            return WeightFingerprint::Unknown {
                weight,
                reason,
                expr_hash,
            }
        }
    };

    let symbol = Symbol::from_terms(terms);
    let (coords, residual) = match mpl_symbol::space::reduce_to_basis(&symbol, &basis, &alphabet) {
        Ok(result) => result,
        Err(err) => {
            return WeightFingerprint::Unknown {
                weight,
                reason: map_symbol_error(err),
                expr_hash,
            }
        }
    };

    let coords_hash = coeffs_hash(&coords);
    let resid_hash = symbol_hash(&residual);

    if residual.is_zero() {
        WeightFingerprint::Integrable {
            weight,
            basis_id,
            coords_hash,
            resid_hash,
        }
    } else {
        WeightFingerprint::NonIntegrable {
            weight,
            basis_id,
            coords_hash,
            resid_hash,
        }
    }
}

fn build_alphabet(terms: &[(Word, Rational64)]) -> (Alphabet, u64) {
    let mut letters: BTreeMap<String, Expr> = BTreeMap::new();
    for (word, coeff) in terms {
        if coeff.is_zero() {
            continue;
        }
        for letter in word.letters() {
            let normalized = letter.normalize();
            let key = normalized.to_canonical_string();
            letters.entry(key).or_insert(normalized);
        }
    }

    let mut alpha_hash = StableHasher::new();
    alpha_hash.update_str("alphabet");
    alpha_hash.update_u64(letters.len() as u64);
    for name in letters.keys() {
        alpha_hash.update_str(name);
    }
    let alpha_hash = alpha_hash.finish();

    let letter_names: Vec<String> = letters.keys().cloned().collect();
    let letter_exprs: Vec<Expr> = letters.values().cloned().collect();
    let weight = terms
        .first()
        .map(|(word, _)| word.letters().len())
        .unwrap_or(0);

    (
        Alphabet::new(format!("fp_weight_{weight}"), letter_exprs, letter_names),
        alpha_hash,
    )
}

fn basis_cached(
    key: &BasisKey,
    alphabet: &Alphabet,
    constraints: &WordConstraints,
    cache: &FingerprintCache,
) -> Result<Arc<Basis>, UnknownReason> {
    let read_guard = match cache.basis.read() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };
    if let Some(basis) = read_guard.get(key) {
        return Ok(basis.clone());
    }
    drop(read_guard);

    let basis = mpl_symbol::space::build_integrable_basis(alphabet, constraints, key.weight)
        .map_err(map_symbol_error)?;
    let basis = Arc::new(basis);
    let mut write_guard = match cache.basis.write() {
        Ok(guard) => guard,
        Err(poison) => poison.into_inner(),
    };
    write_guard.insert(key.clone(), basis.clone());
    Ok(basis)
}

fn coeffs_hash(coeffs: &[Rational64]) -> u64 {
    let mut hasher = StableHasher::new();
    hasher.update_str("coords");
    hasher.update_u64(coeffs.len() as u64);
    for coeff in coeffs {
        hash_rational(&mut hasher, coeff);
    }
    hasher.finish()
}

fn symbol_hash(symbol: &Symbol) -> u64 {
    let mut hasher = StableHasher::new();
    let terms: Vec<_> = symbol.terms().collect();
    hasher.update_str("symbol");
    hasher.update_u64(terms.len() as u64);
    for (word, coeff) in terms {
        hash_rational(&mut hasher, coeff);
        hasher.update_u64(word.letters().len() as u64);
        for letter in word.letters() {
            let normalized = letter.normalize();
            hasher.update_str(&normalized.to_canonical_string());
        }
    }
    hasher.finish()
}

fn hash_rational(hasher: &mut StableHasher, value: &Rational64) {
    hasher.update_i64(*value.numer());
    hasher.update_i64(*value.denom());
}

fn fingerprint_cfg_hash(cfg: &FingerprintConfig) -> u64 {
    let mut hasher = StableHasher::new();
    hasher.update_str("fingerprint_cfg");
    match cfg.weight_limit {
        Some(limit) => {
            hasher.update_u64(1);
            hasher.update_u64(limit as u64);
        }
        None => hasher.update_u64(0),
    }
    hasher.update_u64(cfg.budget.fuel);
    match cfg.budget.time_limit_ms {
        Some(ms) => {
            hasher.update_u64(1);
            hasher.update_u64(ms);
        }
        None => hasher.update_u64(0),
    }
    hasher.update_u64(constraints_hash(&cfg.constraints));
    hasher.finish()
}

fn constraints_hash(constraints: &WordConstraints) -> u64 {
    let mut hasher = StableHasher::new();
    hasher.update_str("constraints");
    match &constraints.first_allowed {
        Some(first) => {
            hasher.update_u64(1);
            hasher.update_u64(first.len() as u64);
            for idx in first {
                hasher.update_u64(*idx as u64);
            }
        }
        None => hasher.update_u64(0),
    }
    match &constraints.allowed_pairs {
        Some(pairs) => {
            hasher.update_u64(1);
            hasher.update_u64(pairs.len() as u64);
            for row in pairs {
                hasher.update_u64(row.len() as u64);
                for &allowed in row {
                    hasher.update_u64(u64::from(allowed));
                }
            }
        }
        None => hasher.update_u64(0),
    }
    hasher.finish()
}

fn basis_id(weight: usize, alphabet_hash: u64, constraints_hash: u64) -> u64 {
    let mut hasher = StableHasher::new();
    hasher.update_str("basis");
    hasher.update_u64(weight as u64);
    hasher.update_u64(alphabet_hash);
    hasher.update_u64(constraints_hash);
    hasher.finish()
}

fn map_symbol_error(err: SymbolError) -> UnknownReason {
    match err {
        SymbolError::NotImplemented(_) => UnknownReason::SymbolNotImplemented,
        SymbolError::Eval(_) => UnknownReason::SymbolEval,
        SymbolError::InsufficientSamples => UnknownReason::InsufficientSamples,
        SymbolError::FuelExhausted => UnknownReason::BudgetExhausted,
        SymbolError::ConstraintBudgetExceeded(_) => UnknownReason::BudgetExhausted,
    }
}

#[cfg(test)]
mod tests {
    use mpl_ir::parse_sexpr;

    use crate::cache::FingerprintCache;
    use crate::error::{Fingerprint, UnknownReason};
    use crate::fingerprint::{fingerprint_expr, FingerprintBudget, FingerprintConfig};
    use mpl_symbol::space::WordConstraints;

    #[test]
    fn fingerprint_stable_repeated() {
        let expr = parse_sexpr("(li2 x)").unwrap().normalize();
        let cfg = FingerprintConfig {
            weight_limit: None,
            budget: FingerprintBudget {
                fuel: 10,
                time_limit_ms: None,
            },
            constraints: WordConstraints::default(),
        };
        let cache = FingerprintCache::new();
        let first = fingerprint_expr(&expr, &cfg, &cache).unwrap();
        for _ in 0..10 {
            let next = fingerprint_expr(&expr, &cfg, &cache).unwrap();
            assert_eq!(first, next);
        }
    }

    #[test]
    fn fuel_zero_returns_unknown() {
        let expr = parse_sexpr("(li2 x)").unwrap().normalize();
        let cfg = FingerprintConfig {
            weight_limit: None,
            budget: FingerprintBudget {
                fuel: 0,
                time_limit_ms: None,
            },
            constraints: WordConstraints::default(),
        };
        let cache = FingerprintCache::new();
        let fp = fingerprint_expr(&expr, &cfg, &cache).unwrap();
        match fp {
            Fingerprint::Unknown { reason, .. } => {
                assert_eq!(reason, UnknownReason::BudgetExhausted);
            }
            other => panic!("unexpected fingerprint: {other:?}"),
        }
    }
}
