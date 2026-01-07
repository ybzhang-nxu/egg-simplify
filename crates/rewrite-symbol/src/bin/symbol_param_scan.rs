use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use mpl_ir::{parse_sexpr, Expr};
use mpl_rewrite::{RewriteConfig, RewriteMode};
use mpl_rewrite_symbol::{
    fingerprint_expr, Fingerprint, FingerprintBudget, FingerprintCache, FingerprintConfig,
    GuardConfig, PenaltyConfig, SymbolContext, SymbolRewriteConfig, UnknownReason,
};
use mpl_symbol::space::WordConstraints;

#[derive(Clone, Debug)]
struct ScanResult {
    expr: &'static str,
    iters: usize,
    node_limit: usize,
    fuel: u64,
    output: String,
    ast_size: usize,
    fp_kind: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let exprs = [
        "(+ (* x y) (* x z))",
        "(+ (+ x y) z)",
        "(* (* x y) z)",
        "(li2 x)",
        "(* (log x) (log y))",
        "(+ 7 (* (log x) (log y) (log z)))",
    ];
    let iters_grid = [3_usize, 10_usize];
    let node_limits = [2000_usize, 10_000_usize];
    let fuel_grid = [0_u64, 20_u64, 100_u64, 1000_u64];

    let cache = Arc::new(FingerprintCache::new());
    let mut results = Vec::new();

    for &expr_str in &exprs {
        let expr = parse_sexpr(expr_str)?.normalize();
        for &iters in &iters_grid {
            for &node_limit in &node_limits {
                for &fuel in &fuel_grid {
                    let ctx = Arc::new(SymbolContext {
                        fp_cfg: FingerprintConfig {
                            weight_limit: None,
                            budget: FingerprintBudget {
                                fuel,
                                time_limit_ms: None,
                            },
                            constraints: WordConstraints::default(),
                        },
                        guard: GuardConfig,
                        penalty: PenaltyConfig::default(),
                        cache: cache.clone(),
                    });
                    let cfg = SymbolRewriteConfig {
                        rewrite: RewriteConfig {
                            iters,
                            node_limit,
                            time_limit_ms: 300,
                            mode: RewriteMode::Aggressive,
                        },
                        ctx: ctx.clone(),
                    };
                    let simplified = mpl_rewrite_symbol::simplify_symbol_aware(&expr, &cfg)?;
                    let output = simplified.to_canonical_string();
                    let ast_size = expr_ast_size(&simplified);
                    let fp_kind = fingerprint_kind(&simplified, &ctx);
                    results.push(ScanResult {
                        expr: expr_str,
                        iters,
                        node_limit,
                        fuel,
                        output,
                        ast_size,
                        fp_kind,
                    });
                }
            }
        }
    }

    let reports_dir = PathBuf::from("reports");
    fs::create_dir_all(&reports_dir)?;
    let csv_path = reports_dir.join("phase2_symbol_scan.csv");
    let md_path = reports_dir.join("phase2_symbol_scan.md");

    let csv = render_csv(&results);
    fs::write(&csv_path, csv)?;

    let md = render_markdown(&results, &exprs, "reports/phase2_symbol_scan.csv");
    fs::write(&md_path, md)?;

    println!("wrote reports/phase2_symbol_scan.md and reports/phase2_symbol_scan.csv");
    Ok(())
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

fn fingerprint_kind(expr: &Expr, ctx: &Arc<SymbolContext>) -> String {
    let fp = fingerprint_expr(expr, &ctx.fp_cfg, &ctx.cache).unwrap_or_else(|_| {
        let key = ctx.cache.expr_key(expr);
        Fingerprint::Unknown {
            reason: UnknownReason::UnsupportedNode,
            expr_hash: key.hash,
        }
    });
    match fp {
        Fingerprint::Unknown { reason, .. } => format!("Unknown({})", reason_name(&reason)),
        Fingerprint::Conflict { .. } => "Conflict".to_string(),
        Fingerprint::ByWeight(map) => {
            let keys: Vec<String> = map
                .keys()
                .filter(|weight| **weight != 0)
                .map(|weight| weight.to_string())
                .collect();
            format!("ByWeight(keys_excluding_0=[{}])", keys.join(","))
        }
    }
}

fn reason_name(reason: &UnknownReason) -> &'static str {
    match reason {
        UnknownReason::SymbolNotImplemented => "SymbolNotImplemented",
        UnknownReason::SymbolEval => "SymbolEval",
        UnknownReason::InsufficientSamples => "InsufficientSamples",
        UnknownReason::BudgetExhausted => "BudgetExhausted",
        UnknownReason::InvalidExponent => "InvalidExponent",
        UnknownReason::InvalidArity => "InvalidArity",
        UnknownReason::UnsupportedNode => "UnsupportedNode",
    }
}

fn render_markdown(results: &[ScanResult], exprs: &[&str], csv_path: &str) -> String {
    let mut out = String::new();
    out.push_str("# Phase 2 symbol-aware scan (deterministic)\n\n");
    out.push_str("Grid:\n\n");
    out.push_str("- iters: 3, 10\n");
    out.push_str("- node_limit: 2000, 10000\n");
    out.push_str("- symbol_fuel: 0, 20, 100, 1000\n\n");
    out.push_str("Full results: `");
    out.push_str(csv_path);
    out.push_str("`\n\n");

    for &expr in exprs {
        out.push_str("## Expr: `");
        out.push_str(expr);
        out.push_str("`\n\n");
        out.push_str("| rank | iters | node_limit | fuel | ast_size | fp_kind | out_expr |\n");
        out.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
        let mut subset: Vec<&ScanResult> = results.iter().filter(|r| r.expr == expr).collect();
        subset.sort_by(|a, b| {
            a.ast_size
                .cmp(&b.ast_size)
                .then_with(|| a.output.cmp(&b.output))
                .then_with(|| a.iters.cmp(&b.iters))
                .then_with(|| a.node_limit.cmp(&b.node_limit))
                .then_with(|| a.fuel.cmp(&b.fuel))
        });
        for (idx, row) in subset.into_iter().take(3).enumerate() {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | `{}` |\n",
                idx + 1,
                row.iters,
                row.node_limit,
                row.fuel,
                row.ast_size,
                row.fp_kind,
                row.output
            ));
        }
        out.push('\n');
    }

    out
}

fn render_csv(results: &[ScanResult]) -> String {
    let mut out = String::new();
    out.push_str("expr,iters,node_limit,symbol_fuel,ast_size,fp_kind,out_expr\n");
    for row in results {
        out.push_str(&format!(
            "\"{}\",{},{},{},{},\"{}\",\"{}\"\n",
            escape_csv(row.expr),
            row.iters,
            row.node_limit,
            row.fuel,
            row.ast_size,
            escape_csv(&row.fp_kind),
            escape_csv(&row.output)
        ));
    }
    out
}

fn escape_csv(value: &str) -> String {
    value.replace('"', "\"\"")
}
