use std::sync::Arc;

use mpl_ir::parse_sexpr;
use mpl_rewrite::RewriteMode;
use mpl_symbol::space::WordConstraints;

use mpl_rewrite_symbol::{
    simplify_symbol_aware, FingerprintBudget, FingerprintCache, FingerprintConfig, GuardConfig,
    PenaltyConfig, SymbolContext, SymbolRewriteConfig,
};

fn main() {
    let args = std::env::args().skip(1);
    let mut aggressive = false;
    let mut expr_arg: Option<String> = None;

    for arg in args {
        if arg == "--aggressive" {
            aggressive = true;
            continue;
        }
        if expr_arg.is_none() {
            expr_arg = Some(arg);
        }
    }

    let expr_str = match expr_arg {
        Some(value) => value,
        None => {
            eprintln!("usage: symbol_simplify [--aggressive] '<expr>'");
            std::process::exit(2);
        }
    };

    let parsed = match parse_sexpr(&expr_str) {
        Ok(expr) => expr,
        Err(err) => {
            eprintln!("parse error: {err}");
            std::process::exit(1);
        }
    };

    let ctx = Arc::new(SymbolContext {
        fp_cfg: FingerprintConfig {
            weight_limit: None,
            budget: FingerprintBudget {
                fuel: 50,
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
            iters: 12,
            node_limit: 50_000,
            time_limit_ms: 200,
            mode: if aggressive {
                RewriteMode::Aggressive
            } else {
                RewriteMode::Safe
            },
        },
        ctx,
    };

    match simplify_symbol_aware(&parsed, &cfg) {
        Ok(expr) => {
            println!("{}", expr.to_canonical_string());
        }
        Err(err) => {
            eprintln!("simplify error: {err}");
            std::process::exit(1);
        }
    }
}
