use clap::{Parser, Subcommand};
use mpl_ir::parse_sexpr;

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
        Commands::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
