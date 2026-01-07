use std::sync::Arc;

use mpl_ir::{parse_sexpr, Expr};
use mpl_rewrite::{simplify_algebra, RewriteConfig, RewriteMode};
use mpl_rewrite_symbol::{
    simplify_symbol_aware, FingerprintBudget, FingerprintCache, FingerprintConfig, GuardConfig,
    PenaltyConfig, SymbolContext, SymbolRewriteConfig,
};
use mpl_symbol::space::{check_integrable_n, WordConstraints};
use mpl_symbol::{check_integrable, symbol};

pub struct SimplifyOptions {
    pub iters: usize,
    pub node_limit: usize,
    pub time_limit_ms: u64,
    pub aggressive: bool,
    pub no_rewrite: bool,
    pub no_symbol_guard: bool,
    pub symbol_aware: bool,
    pub symbol_fuel: Option<u64>,
    pub symbol_weight_limit: Option<usize>,
    pub unknown_penalty: Option<u64>,
    pub non_integrable_penalty: Option<u64>,
    pub conflict_penalty: Option<u64>,
}

pub fn simplify_expr(input: &str, opts: &SimplifyOptions) -> Result<Expr, String> {
    let parsed = parse_sexpr(input).map_err(|err| err.to_string())?;
    let baseline = parsed.normalize();

    if opts.no_rewrite {
        return Ok(baseline);
    }

    let cfg = RewriteConfig {
        iters: opts.iters,
        node_limit: opts.node_limit,
        time_limit_ms: opts.time_limit_ms,
        mode: if opts.aggressive {
            RewriteMode::Aggressive
        } else {
            RewriteMode::Safe
        },
    };

    if opts.symbol_aware {
        let symbol_fuel = opts.symbol_fuel.unwrap_or(100);
        let penalty_defaults = PenaltyConfig::default();
        let penalty = PenaltyConfig {
            unknown_penalty: opts
                .unknown_penalty
                .unwrap_or(penalty_defaults.unknown_penalty),
            non_integrable_penalty: opts
                .non_integrable_penalty
                .unwrap_or(penalty_defaults.non_integrable_penalty),
            conflict_penalty: opts
                .conflict_penalty
                .unwrap_or(penalty_defaults.conflict_penalty),
        };
        let ctx = Arc::new(SymbolContext {
            fp_cfg: FingerprintConfig {
                weight_limit: opts.symbol_weight_limit,
                budget: FingerprintBudget {
                    fuel: symbol_fuel,
                    time_limit_ms: None,
                },
                constraints: WordConstraints::default(),
            },
            guard: GuardConfig,
            penalty,
            cache: Arc::new(FingerprintCache::new()),
        });
        let cfg = SymbolRewriteConfig { rewrite: cfg, ctx };
        let simplified = simplify_symbol_aware(&baseline, &cfg).map_err(|err| err.to_string())?;
        return Ok(simplified);
    }

    let candidate = simplify_algebra(&baseline, &cfg).map_err(|err| err.to_string())?;

    if opts.no_symbol_guard {
        return Ok(candidate);
    }

    match (symbol(&baseline), symbol(&candidate)) {
        (Ok(sb), Ok(sc)) => {
            if sb != sc {
                return Ok(baseline);
            }
            let max_weight = sc
                .terms()
                .map(|(word, _coeff)| word.letters().len())
                .max()
                .unwrap_or(0);
            let integrable = if max_weight <= 2 {
                check_integrable(&sc)
            } else {
                check_integrable_n(&sc)
            };
            match integrable {
                Ok(true) => Ok(candidate),
                Ok(false) => Ok(baseline),
                Err(_) => Ok(candidate),
            }
        }
        _ => Ok(candidate),
    }
}
