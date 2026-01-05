use mpl_ir::{parse_sexpr, Expr};
use mpl_rewrite::{simplify_algebra, RewriteConfig, RewriteMode};
use mpl_symbol::{check_integrable, symbol};

pub struct SimplifyOptions {
    pub iters: usize,
    pub node_limit: usize,
    pub time_limit_ms: u64,
    pub aggressive: bool,
    pub no_rewrite: bool,
    pub no_symbol_guard: bool,
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
    let candidate = simplify_algebra(&baseline, &cfg).map_err(|err| err.to_string())?;

    if opts.no_symbol_guard {
        return Ok(candidate);
    }

    match (symbol(&baseline), symbol(&candidate)) {
        (Ok(sb), Ok(sc)) => {
            if sb != sc {
                return Ok(baseline);
            }
            match check_integrable(&sc) {
                Ok(true) => Ok(candidate),
                Ok(false) => Ok(baseline),
                Err(_) => Ok(candidate),
            }
        }
        _ => Ok(candidate),
    }
}
