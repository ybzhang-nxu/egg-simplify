use std::sync::Arc;

use egg::{Analysis, DidMerge, EGraph, Id};
use mpl_ir::Expr;
use num_rational::Rational64;
use num_traits::Zero;

use mpl_rewrite::lang::Lang;

use crate::cache::ExprKey;
use crate::error::{Fingerprint, UnknownReason};
use crate::fingerprint::fingerprint_expr;
use crate::hash::StableHasher;
use crate::SymbolContext;

/// egg analysis that annotates eclasses with symbol fingerprints.
#[derive(Clone)]
pub struct SymbolAnalysis {
    /// Shared symbol context for fingerprinting and penalties.
    pub ctx: Arc<SymbolContext>,
}

/// Analysis data stored on each eclass.
#[derive(Clone, Debug)]
pub struct SymData {
    /// Deterministic representative expression.
    pub repr: Expr,
    /// Cached canonical key for the representative.
    pub repr_key: ExprKey,
    /// Fingerprint computed from the representative.
    pub fingerprint: Fingerprint,
    /// Constant value if the eclass is purely numeric.
    pub const_value: Option<Rational64>,
}

impl Analysis<Lang> for SymbolAnalysis {
    type Data = SymData;

    fn make(egraph: &EGraph<Lang, Self>, enode: &Lang) -> Self::Data {
        let ctx = &egraph.analysis.ctx;
        let (repr, invalid_pow) = repr_from_enode(egraph, enode);
        let repr_norm = repr.normalize();
        let repr_key = ctx.cache.expr_key(&repr_norm);

        let fingerprint = if invalid_pow {
            Fingerprint::Unknown {
                reason: UnknownReason::InvalidExponent,
                expr_hash: repr_key.hash,
            }
        } else {
            match fingerprint_expr(&repr_norm, &ctx.fp_cfg, &ctx.cache) {
                Ok(fp) => fp,
                Err(_) => Fingerprint::Unknown {
                    reason: UnknownReason::UnsupportedNode,
                    expr_hash: repr_key.hash,
                },
            }
        };

        let const_value = const_value_from_enode(egraph, enode);

        SymData {
            repr: repr_norm,
            repr_key,
            fingerprint,
            const_value,
        }
    }

    fn merge(&mut self, to: &mut Self::Data, from: Self::Data) -> DidMerge {
        let mut did_merge = false;

        let to_fp = to.fingerprint.clone();
        let merged_fp = merge_fingerprints(&to_fp, &from.fingerprint);
        if merged_fp != to.fingerprint {
            to.fingerprint = merged_fp;
            did_merge = true;
        }

        let choose_from = prefer_repr(&to.repr, &to.repr_key, &from.repr, &from.repr_key);
        if choose_from {
            to.repr = from.repr;
            to.repr_key = from.repr_key;
            did_merge = true;
        }

        match (&to.const_value, &from.const_value) {
            (Some(left), Some(right)) => {
                if left != right {
                    to.const_value = None;
                    did_merge = true;
                }
            }
            (None, Some(value)) => {
                to.const_value = Some(*value);
                did_merge = true;
            }
            _ => {}
        }

        DidMerge(did_merge, false)
    }

    fn modify(_egraph: &mut EGraph<Lang, Self>, _id: Id) {
        // Phase 1: do not union in modify.
    }
}

fn prefer_repr(left: &Expr, left_key: &ExprKey, right: &Expr, right_key: &ExprKey) -> bool {
    let left_size = expr_ast_size(left);
    let right_size = expr_ast_size(right);
    right_size < left_size || (right_size == left_size && right_key.canon < left_key.canon)
}

fn expr_ast_size(expr: &Expr) -> usize {
    match expr {
        Expr::Rational(_) | Expr::Var(_) => 1,
        Expr::Neg(inner) | Expr::Log(inner) | Expr::Li2(inner) => 1 + expr_ast_size(inner),
        Expr::Pow(base, _) => 1 + expr_ast_size(base),
        Expr::Add(children) | Expr::Mul(children) => {
            1 + children.iter().map(expr_ast_size).sum::<usize>()
        }
    }
}

fn repr_from_enode(egraph: &EGraph<Lang, SymbolAnalysis>, enode: &Lang) -> (Expr, bool) {
    match enode {
        Lang::Num(n) => (Expr::Rational(*n), false),
        Lang::Var(sym) => (Expr::Var(sym.to_string()), false),
        Lang::Add([a, b]) => {
            let left = egraph[*a].data.repr.clone();
            let right = egraph[*b].data.repr.clone();
            (Expr::Add(vec![left, right]), false)
        }
        Lang::Mul([a, b]) => {
            let left = egraph[*a].data.repr.clone();
            let right = egraph[*b].data.repr.clone();
            (Expr::Mul(vec![left, right]), false)
        }
        Lang::Log(inner) => {
            let child = egraph[*inner].data.repr.clone();
            (Expr::Log(Box::new(child)), false)
        }
        Lang::Li2(inner) => {
            let child = egraph[*inner].data.repr.clone();
            (Expr::Li2(Box::new(child)), false)
        }
        Lang::Pow([base, exp]) => {
            let base_expr = egraph[*base].data.repr.clone();
            let exp_value =
                egraph[*exp]
                    .data
                    .const_value
                    .or_else(|| match &egraph[*exp].data.repr {
                        Expr::Rational(value) => Some(*value),
                        _ => None,
                    });
            if let Some(exp_r) = exp_value {
                if exp_r.is_integer() {
                    let numer = *exp_r.numer();
                    if let Ok(exp_int) = i32::try_from(numer) {
                        return (Expr::Pow(Box::new(base_expr), exp_int), false);
                    }
                }
            }
            (
                invalid_pow_placeholder(&base_expr, &egraph[*exp].data.repr),
                true,
            )
        }
    }
}

fn invalid_pow_placeholder(base: &Expr, exp: &Expr) -> Expr {
    let mut hasher = StableHasher::new();
    hasher.update_str("pow_invalid");
    hasher.update_str(&base.to_canonical_string());
    hasher.update_str(&exp.to_canonical_string());
    let digest = hasher.finish();
    Expr::Var(format!("pow_invalid_{digest:016x}"))
}

fn const_value_from_enode(
    egraph: &EGraph<Lang, SymbolAnalysis>,
    enode: &Lang,
) -> Option<Rational64> {
    match enode {
        Lang::Num(n) => Some(*n),
        Lang::Add([a, b]) => Some(egraph[*a].data.const_value? + egraph[*b].data.const_value?),
        Lang::Mul([a, b]) => Some(egraph[*a].data.const_value? * egraph[*b].data.const_value?),
        Lang::Pow([a, b]) => {
            let base = egraph[*a].data.const_value?;
            let exp_r = egraph[*b].data.const_value?;
            if !exp_r.is_integer() {
                return None;
            }
            let exp_num = *exp_r.numer();
            let exp = i32::try_from(exp_num).ok()?;
            if exp < 0 && base.is_zero() {
                return None;
            }
            Some(base.pow(exp))
        }
        Lang::Log(_) | Lang::Li2(_) | Lang::Var(_) => None,
    }
}

fn merge_fingerprints(left: &Fingerprint, right: &Fingerprint) -> Fingerprint {
    if left == right {
        return left.clone();
    }

    match (left, right) {
        (
            Fingerprint::Conflict {
                left_digest: left_a,
                right_digest: left_b,
            },
            Fingerprint::Conflict {
                left_digest: right_a,
                right_digest: right_b,
            },
        ) => {
            let left_pair = (*left_a, *left_b);
            let right_pair = (*right_a, *right_b);
            if left_pair <= right_pair {
                left.clone()
            } else {
                right.clone()
            }
        }
        (Fingerprint::Conflict { .. }, _) => left.clone(),
        (_, Fingerprint::Conflict { .. }) => right.clone(),
        (
            Fingerprint::Unknown {
                reason: left_reason,
                expr_hash: left_hash,
            },
            Fingerprint::Unknown {
                reason: right_reason,
                expr_hash: right_hash,
            },
        ) => {
            let left_key = (reason_tag(left_reason), *left_hash);
            let right_key = (reason_tag(right_reason), *right_hash);
            if left_key <= right_key {
                left.clone()
            } else {
                right.clone()
            }
        }
        (Fingerprint::Unknown { .. }, _) => left.clone(),
        (_, Fingerprint::Unknown { .. }) => right.clone(),
        _ => {
            let left_digest = fingerprint_digest(left);
            let right_digest = fingerprint_digest(right);
            let (min_digest, max_digest) = if left_digest <= right_digest {
                (left_digest, right_digest)
            } else {
                (right_digest, left_digest)
            };
            Fingerprint::Conflict {
                left_digest: min_digest,
                right_digest: max_digest,
            }
        }
    }
}

fn fingerprint_digest(fp: &Fingerprint) -> u64 {
    use crate::error::WeightFingerprint;
    use crate::hash::StableHasher;

    let mut hasher = StableHasher::new();
    match fp {
        Fingerprint::ByWeight(map) => {
            hasher.update_str("by_weight");
            hasher.update_u64(map.len() as u64);
            for (weight, wf) in map {
                hasher.update_u64(*weight as u64);
                match wf {
                    WeightFingerprint::Integrable {
                        basis_id,
                        coords_hash,
                        resid_hash,
                        ..
                    } => {
                        hasher.update_str("integrable");
                        hasher.update_u64(*basis_id);
                        hasher.update_u64(*coords_hash);
                        hasher.update_u64(*resid_hash);
                    }
                    WeightFingerprint::NonIntegrable {
                        basis_id,
                        coords_hash,
                        resid_hash,
                        ..
                    } => {
                        hasher.update_str("non_integrable");
                        hasher.update_u64(*basis_id);
                        hasher.update_u64(*coords_hash);
                        hasher.update_u64(*resid_hash);
                    }
                    WeightFingerprint::Unknown {
                        reason, expr_hash, ..
                    } => {
                        hasher.update_str("unknown");
                        hasher.update_u64(reason_tag(reason));
                        hasher.update_u64(*expr_hash);
                    }
                }
            }
        }
        Fingerprint::Unknown { reason, expr_hash } => {
            hasher.update_str("unknown");
            hasher.update_u64(reason_tag(reason));
            hasher.update_u64(*expr_hash);
        }
        Fingerprint::Conflict {
            left_digest,
            right_digest,
        } => {
            hasher.update_str("conflict");
            hasher.update_u64(*left_digest);
            hasher.update_u64(*right_digest);
        }
    }
    hasher.finish()
}

fn reason_tag(reason: &UnknownReason) -> u64 {
    match reason {
        UnknownReason::SymbolNotImplemented => 1,
        UnknownReason::SymbolEval => 2,
        UnknownReason::InsufficientSamples => 3,
        UnknownReason::BudgetExhausted => 4,
        UnknownReason::InvalidExponent => 5,
        UnknownReason::InvalidArity => 6,
        UnknownReason::UnsupportedNode => 7,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use egg::Analysis;
    use mpl_ir::parse_sexpr;

    use super::{expr_ast_size, SymData, SymbolAnalysis};
    use crate::error::{Fingerprint, UnknownReason};
    use crate::{FingerprintCache, FingerprintConfig, GuardConfig, PenaltyConfig, SymbolContext};
    use mpl_symbol::space::WordConstraints;

    fn ctx() -> Arc<SymbolContext> {
        Arc::new(SymbolContext {
            fp_cfg: FingerprintConfig {
                weight_limit: None,
                budget: crate::FingerprintBudget {
                    fuel: 1,
                    time_limit_ms: None,
                },
                constraints: WordConstraints::default(),
            },
            guard: GuardConfig,
            penalty: PenaltyConfig::default(),
            cache: Arc::new(FingerprintCache::new()),
        })
    }

    #[test]
    fn expr_ast_size_counts_nodes() {
        let expr = parse_sexpr("(+ x (* y z))").unwrap().normalize();
        assert_eq!(expr_ast_size(&expr), 5);
    }

    #[test]
    fn merge_prefers_smaller_repr_then_lex() {
        let ctx = ctx();
        let cache = ctx.cache.clone();
        let mut analysis = SymbolAnalysis { ctx };

        let expr_small = parse_sexpr("x").unwrap().normalize();
        let expr_big = parse_sexpr("(+ x y)").unwrap().normalize();

        let mut to = SymData {
            repr: expr_big.clone(),
            repr_key: cache.expr_key(&expr_big),
            fingerprint: Fingerprint::Unknown {
                reason: UnknownReason::UnsupportedNode,
                expr_hash: 1,
            },
            const_value: None,
        };
        let from = SymData {
            repr: expr_small.clone(),
            repr_key: cache.expr_key(&expr_small),
            fingerprint: Fingerprint::Unknown {
                reason: UnknownReason::UnsupportedNode,
                expr_hash: 2,
            },
            const_value: None,
        };

        analysis.merge(&mut to, from);
        assert_eq!(to.repr.to_canonical_string(), "x");
    }
}
