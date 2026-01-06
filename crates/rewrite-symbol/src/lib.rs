mod analysis;
mod cache;
mod error;
mod extractor;
mod fingerprint;
mod hash;

use std::sync::Arc;
use std::time::Duration;

use egg::{Extractor, Runner};
use mpl_ir::Expr;
use mpl_rewrite::{lang::Lang, lift_expr, lower_expr, RewriteConfig};

use crate::extractor::SymbolCostFn;

pub use analysis::{SymData, SymbolAnalysis};
pub use cache::{BasisKey, ExprKey, FingerprintCache, FingerprintKey};
pub use error::{Fingerprint, RewriteSymbolError, UnknownReason, WeightFingerprint};
pub use extractor::PenaltyConfig;
pub use fingerprint::{fingerprint_expr, FingerprintBudget, FingerprintConfig};
pub use hash::{stable_hash_bytes, stable_hash_str};

/// Placeholder for future guard configuration.
#[derive(Clone, Debug, Default)]
pub struct GuardConfig;

/// Shared context for symbol-aware rewriting.
#[derive(Clone, Debug)]
pub struct SymbolContext {
    /// Fingerprint configuration.
    pub fp_cfg: FingerprintConfig,
    /// Guard configuration (Phase 1 placeholder).
    pub guard: GuardConfig,
    /// Penalty configuration for the extractor.
    pub penalty: PenaltyConfig,
    /// Shared fingerprint cache.
    pub cache: Arc<FingerprintCache>,
}

/// Configuration for symbol-aware rewriting.
#[derive(Clone, Debug)]
pub struct SymbolRewriteConfig {
    /// Base rewrite configuration.
    pub rewrite: RewriteConfig,
    /// Shared symbol context.
    pub ctx: Arc<SymbolContext>,
}

/// Simplify an expression using symbol-aware extraction.
pub fn simplify_symbol_aware(
    expr: &Expr,
    cfg: &SymbolRewriteConfig,
) -> Result<Expr, RewriteSymbolError> {
    let baseline = expr.normalize();
    let lowered = lower_expr(&baseline)?;

    let analysis = SymbolAnalysis {
        ctx: cfg.ctx.clone(),
    };

    let rules = mpl_rewrite::rules::rules_for_mode::<SymbolAnalysis>(cfg.rewrite.mode);
    let runner = Runner::<Lang, SymbolAnalysis, ()>::new(analysis)
        .with_expr(&lowered)
        .with_iter_limit(cfg.rewrite.iters)
        .with_node_limit(cfg.rewrite.node_limit)
        .with_time_limit(Duration::from_millis(cfg.rewrite.time_limit_ms))
        .run(rules.iter());

    let root = runner.roots[0];
    let cost_fn = SymbolCostFn::new(&runner.egraph, cfg.ctx.penalty.clone());
    let extractor = Extractor::new(&runner.egraph, cost_fn);
    let (_cost, best) = extractor.find_best(root);

    let lifted = lift_expr(&best)?;
    Ok(lifted.normalize())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use mpl_ir::parse_sexpr;
    use mpl_rewrite::RewriteMode;
    use mpl_symbol::space::WordConstraints;

    use super::{simplify_symbol_aware, FingerprintBudget, FingerprintConfig, GuardConfig};
    use super::{FingerprintCache, PenaltyConfig, SymbolContext, SymbolRewriteConfig};

    #[test]
    fn simplify_smoke() {
        let expr = parse_sexpr("(+ (* x y) (* x z))").unwrap().normalize();
        let ctx = Arc::new(SymbolContext {
            fp_cfg: FingerprintConfig {
                weight_limit: None,
                budget: FingerprintBudget {
                    fuel: 10,
                    time_limit_ms: None,
                },
                constraints: WordConstraints::default(),
            },
            guard: GuardConfig,
            penalty: PenaltyConfig::default(),
            cache: Arc::new(FingerprintCache::new()),
        });
        let cfg = SymbolRewriteConfig {
            rewrite: mpl_rewrite::RewriteConfig {
                iters: 10,
                node_limit: 50_000,
                time_limit_ms: 200,
                mode: RewriteMode::Aggressive,
            },
            ctx,
        };
        let simplified = simplify_symbol_aware(&expr, &cfg).unwrap();
        assert!(!simplified.to_canonical_string().is_empty());
    }
}
