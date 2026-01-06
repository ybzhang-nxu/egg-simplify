mod pipeline;

use clap::{Parser, Subcommand};
use mpl_ir::parse_sexpr;
use mpl_symbol::{check_integrable, symbol, Coeff};
use pipeline::{simplify_expr, SimplifyOptions};

#[derive(Parser)]
#[command(name = "mpl-simplify")]
#[command(about = "Minimal MPL simplifier v0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Normalize a single s-expression and print its canonical form.
    Normalize {
        #[arg(long)]
        expr: String,
    },
    /// Compute the symbol of an expression.
    Symbol {
        #[arg(long)]
        expr: String,
    },
    /// Check weight-2 integrability of an expression's symbol.
    CheckIntegrable {
        #[arg(long)]
        expr: String,
    },
    /// Simplify an expression using the rewrite engine.
    Simplify {
        #[arg(long)]
        expr: String,
        #[arg(long, default_value_t = 20)]
        iters: usize,
        #[arg(long, default_value_t = 50_000)]
        node_limit: usize,
        #[arg(long, default_value_t = 300)]
        time_limit_ms: u64,
        #[arg(long)]
        aggressive: bool,
        #[arg(long)]
        no_rewrite: bool,
        #[arg(long)]
        no_symbol_guard: bool,
        #[arg(long)]
        symbol_aware: bool,
        #[arg(long, requires = "symbol_aware")]
        symbol_fuel: Option<u64>,
        #[arg(long, requires = "symbol_aware")]
        symbol_weight_limit: Option<usize>,
        #[arg(long, requires = "symbol_aware")]
        unknown_penalty: Option<u64>,
        #[arg(long, requires = "symbol_aware")]
        non_integrable_penalty: Option<u64>,
        #[arg(long, requires = "symbol_aware")]
        conflict_penalty: Option<u64>,
    },
    /// Print version information.
    Version,
}

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Commands::Normalize { expr } => {
            let parsed = parse_sexpr(&expr).map_err(|err| err.to_string())?;
            let normalized = parsed.normalize();
            println!("{}", normalized.to_canonical_string());
            Ok(())
        }
        Commands::Symbol { expr } => {
            let parsed = parse_sexpr(&expr).map_err(|err| err.to_string())?;
            let normalized = parsed.normalize();
            let sym = symbol(&normalized).map_err(|err| err.to_string())?;
            if sym.is_zero() {
                println!("0");
                return Ok(());
            }
            for (word, coeff) in sym.terms() {
                let letters = word
                    .letters()
                    .iter()
                    .map(|expr| expr.to_canonical_string())
                    .collect::<Vec<_>>()
                    .join(" ⊗ ");
                println!("{} * ({})", format_rational(coeff), letters);
            }
            Ok(())
        }
        Commands::CheckIntegrable { expr } => {
            let parsed = parse_sexpr(&expr).map_err(|err| err.to_string())?;
            let normalized = parsed.normalize();
            let sym = symbol(&normalized).map_err(|err| err.to_string())?;
            let result = check_integrable(&sym).map_err(|err| err.to_string())?;
            println!("{}", if result { "true" } else { "false" });
            Ok(())
        }
        Commands::Simplify {
            expr,
            iters,
            node_limit,
            time_limit_ms,
            aggressive,
            no_rewrite,
            no_symbol_guard,
            symbol_aware,
            symbol_fuel,
            symbol_weight_limit,
            unknown_penalty,
            non_integrable_penalty,
            conflict_penalty,
        } => {
            let opts = SimplifyOptions {
                iters,
                node_limit,
                time_limit_ms,
                aggressive,
                no_rewrite,
                no_symbol_guard,
                symbol_aware,
                symbol_fuel,
                symbol_weight_limit,
                unknown_penalty,
                non_integrable_penalty,
                conflict_penalty,
            };
            let simplified = simplify_expr(&expr, &opts)?;
            println!("{}", simplified.to_canonical_string());
            Ok(())
        }
        Commands::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

fn format_rational(value: &Coeff) -> String {
    let numer = *value.numer();
    let denom = *value.denom();
    if denom == 1 {
        numer.to_string()
    } else {
        format!("{numer}/{denom}")
    }
}
