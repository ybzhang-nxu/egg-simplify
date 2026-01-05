pub mod config;
pub mod extract;
pub mod lang;
pub mod lower;
pub mod rules;

use std::time::Duration;

use egg::Runner;
use mpl_ir::Expr;

use crate::extract::extract_best;
use crate::lang::{ConstFold, Lang};
use crate::rules::rules;

pub use config::{RewriteConfig, RewriteError, RewriteMode};
pub use extract::lift_expr;
pub use lower::lower_expr;

pub fn simplify_algebra(expr: &Expr, cfg: &RewriteConfig) -> Result<Expr, RewriteError> {
    let baseline = expr.normalize();
    let lowered = lower_expr(&baseline)?;
    let runner = Runner::<Lang, ConstFold>::default()
        .with_expr(&lowered)
        .with_iter_limit(cfg.iters)
        .with_node_limit(cfg.node_limit)
        .with_time_limit(Duration::from_millis(cfg.time_limit_ms))
        .run(&rules(cfg.mode));
    let best = extract_best(&runner);
    lift_expr(&best)
}

#[cfg(test)]
mod tests {
    use super::{lift_expr, lower_expr, simplify_algebra};
    use crate::config::{RewriteConfig, RewriteMode};
    use mpl_ir::parse_sexpr;

    #[test]
    fn lower_lift_roundtrip() {
        let inputs = [
            "(+ x y 3)",
            "(* 2 x (^ y 3))",
            "(^ x -2)",
            "(log (+ x 1))",
            "(li2 (* x y))",
        ];
        for input in inputs {
            let expr = parse_sexpr(input).expect("parse").normalize();
            let lowered = lower_expr(&expr).expect("lower");
            let lifted = lift_expr(&lowered).expect("lift");
            assert_eq!(lifted.to_canonical_string(), expr.to_canonical_string());
        }
    }

    #[test]
    fn aggressive_factoring_changes_shape() {
        let expr = parse_sexpr("(+ (* x y) (* x z))")
            .expect("parse")
            .normalize();
        let cfg = RewriteConfig {
            iters: 10,
            node_limit: 50_000,
            time_limit_ms: 200,
            mode: RewriteMode::Aggressive,
        };
        let simplified = simplify_algebra(&expr, &cfg).expect("simplify");
        let output = simplified.to_canonical_string();
        assert!(
            output.contains("(+ y z)") && output.contains("x"),
            "unexpected output: {output}"
        );
    }

    #[test]
    fn safe_mode_keeps_simple_form() {
        let expr = parse_sexpr("(+ x 0)").expect("parse").normalize();
        let cfg = RewriteConfig {
            iters: 5,
            node_limit: 10_000,
            time_limit_ms: 50,
            mode: RewriteMode::Safe,
        };
        let simplified = simplify_algebra(&expr, &cfg).expect("simplify");
        assert_eq!(simplified.to_canonical_string(), expr.to_canonical_string());
    }
}
