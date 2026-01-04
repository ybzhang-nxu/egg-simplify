use clap::Parser;

#[derive(Parser)]
#[command(name = "egg-simplify")]
#[command(about = "E-graph based symbolic simplifier (egg)")]
struct Args {
    /// Input expression in s-expression form, e.g. "(+ (* x 1) 0)"
    input: String,

    /// Iteration limit
    #[arg(long, default_value_t = 10)]
    iters: usize,
}

fn main() {
    let args = Args::parse();
    match core::simplify_sexp(&args.input, args.iters) {
        Ok(out) => println!("{out}"),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
