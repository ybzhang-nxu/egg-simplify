use std::path::PathBuf;

use super::{run_kze2_hankel_mvp, Kze2HankelMvpConfig};

pub fn run_kze2_hankel_mvp_cli(args: Vec<String>) -> Result<(), String> {
    let mut cfg = Kze2HankelMvpConfig::default();
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--r" => {
                idx += 1;
                cfg.r = parse_usize(&next_arg(&args, &mut idx, "--r")?, "--r")?;
            }
            "--prime" => {
                idx += 1;
                cfg.prime = parse_u64(&next_arg(&args, &mut idx, "--prime")?, "--prime")?;
            }
            "--prefix-len" => {
                idx += 1;
                cfg.prefix_len =
                    parse_usize(&next_arg(&args, &mut idx, "--prefix-len")?, "--prefix-len")?;
            }
            "--holdout-len" => {
                idx += 1;
                cfg.holdout_len = parse_usize(
                    &next_arg(&args, &mut idx, "--holdout-len")?,
                    "--holdout-len",
                )?;
            }
            "--out-dir" | "--out" => {
                idx += 1;
                cfg.out_dir = Some(PathBuf::from(next_arg(&args, &mut idx, "--out-dir")?));
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

    let report = run_kze2_hankel_mvp(&cfg).map_err(|err| err.to_string())?;
    if let Some(out_dir) = report.out_dir.as_ref() {
        println!("wrote params.json and stats.txt to {}", out_dir.display());
    } else {
        println!(
            "kze2-hankel-mvp hankel_rank={} target_r={} mismatches={}",
            report.hankel_rank, report.r, report.mismatches
        );
    }
    Ok(())
}

fn print_help() {
    println!("mpl-experiments kze2-hankel-mvp");
    println!();
    println!("Usage:");
    println!("  mpl-experiments kze2-hankel-mvp [options]");
    println!();
    println!("Options:");
    println!("  --r <n>             Even dimension (default: 20)");
    println!("  --prime <p>         Prime modulus (default: 1000003)");
    println!("  --prefix-len <n>    Prefix/suffix length L (default: 2)");
    println!("  --holdout-len <n>   Holdout length (default: 6)");
    println!("  --out-dir <dir>     Output directory (optional)");
    println!("  --help              Show this help");
}

fn next_arg(args: &[String], idx: &mut usize, flag: &str) -> Result<String, String> {
    if *idx >= args.len() {
        return Err(format!("missing value after {flag}"));
    }
    let value = args[*idx].clone();
    *idx += 1;
    Ok(value)
}

fn parse_usize(value: &str, flag: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("invalid {flag} value: {value}"))
}

fn parse_u64(value: &str, flag: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("invalid {flag} value: {value}"))
}
