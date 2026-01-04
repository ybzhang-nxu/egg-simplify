use clap::{Parser, Subcommand};
use mpl_ir::parse_sexpr;
use mpl_symbol::{check_integrable, symbol, Coeff};

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
