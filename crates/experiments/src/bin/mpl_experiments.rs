use std::env;
use std::path::PathBuf;

use mpl_experiments::{load_spec, run_count_only, run_experiment, write_count_only, write_outputs};

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(cmd) = args.next() else {
        print_help();
        return Ok(());
    };

    match cmd.as_str() {
        "run" => run_spec(args),
        "count" => count_spec(args),
        "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(format!("unknown subcommand: {other}")),
    }
}

fn run_spec<I>(mut args: I) -> Result<(), String>
where
    I: Iterator<Item = String>,
{
    let mut spec_path: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--spec" => {
                spec_path = Some(PathBuf::from(next_value(&mut args, "--spec")?));
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            other => {
                return Err(format!("unknown arg: {other}"));
            }
        }
    }

    let path = spec_path.ok_or_else(|| "missing --spec <path>".to_string())?;
    let cfg = load_spec(&path).map_err(|err| err.to_string())?;
    let report = run_experiment(&cfg).map_err(|err| err.to_string())?;
    write_outputs(&report, &cfg.out_dir).map_err(|err| err.to_string())?;
    println!(
        "wrote basis_stats.txt, dim_vs_w.csv, pairs.csv, pairs_by_weight.csv, triplets.csv, triplets_by_weight.csv, topology_metrics.csv to {}",
        cfg.out_dir.display()
    );
    Ok(())
}

fn count_spec<I>(mut args: I) -> Result<(), String>
where
    I: Iterator<Item = String>,
{
    let mut spec_path: Option<PathBuf> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--spec" => {
                spec_path = Some(PathBuf::from(next_value(&mut args, "--spec")?));
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            other => {
                return Err(format!("unknown arg: {other}"));
            }
        }
    }

    let path = spec_path.ok_or_else(|| "missing --spec <path>".to_string())?;
    let cfg = load_spec(&path).map_err(|err| err.to_string())?;
    let report = run_count_only(&cfg).map_err(|err| err.to_string())?;
    write_count_only(&report, &cfg.out_dir).map_err(|err| err.to_string())?;
    println!("wrote counts_only.csv to {}", cfg.out_dir.display());
    Ok(())
}

fn next_value<I>(args: &mut I, flag: &str) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    args.next()
        .ok_or_else(|| format!("missing value after {flag}"))
}

fn print_help() {
    println!("mpl-experiments (M1 runner)");
    println!();
    println!("Usage:");
    println!("  mpl-experiments run --spec <path>");
    println!("  mpl-experiments count --spec <path>");
    println!();
    println!("Options:");
    println!("  --spec <path>          Experiment TOML spec");
    println!("  --help                 Show this help");
}
