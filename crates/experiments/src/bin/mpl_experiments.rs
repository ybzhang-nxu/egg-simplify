use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use mpl_experiments::{
    load_filtration_spec, load_spec, prefix_from_names, render_cross_loop_scan_index,
    run_count_only, run_cross_loop, run_cross_loop_scan, run_esymb_hankel_subblock,
    run_esymb_rank_scan, run_esymb_span_deps, run_experiment, run_filtration,
    run_kze2_hankel_mvp_cli, run_ladder_gen, run_path1_toy, run_pentaladder_gen, write_count_only,
    write_cross_loop_outputs, write_cross_loop_scan_outputs, write_filtration_summary,
    write_outputs, AlphabetMode, CoefSet, CrossLoopOptions, CrossLoopScanOptions,
    EsymbHankelSubblockConfig, EsymbRankScanConfig, EsymbSpanDepsConfig, LadderFamily,
    LadderGenConfig, NormalizeChoice, PairsMode, Path1Mode, Path1ToyConfig, PentaladderFamily,
    PentaladderGenConfig, RowFilter, SpanFamilyFilter, SuffixSpec,
};

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
        "filtration" => run_filtration_spec(args),
        "cross-loop" => cross_loop(args.collect()),
        "esymb-rank-scan" => esymb_rank_scan(args.collect()),
        "esymb-span-deps" => esymb_span_deps(args.collect()),
        "esymb-hankel-subblock" => esymb_hankel_subblock(args.collect()),
        "path1-toy" => path1_toy(args.collect()),
        "gen-ladder" => gen_ladder(args.collect()),
        "gen-pentaladder" => gen_pentaladder(args.collect()),
        "kze2-hankel-mvp" => run_kze2_hankel_mvp_cli(args.collect()),
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
        "wrote basis_stats.txt, dim_vs_w.csv, pairs.csv, pairs_by_weight.csv, triplets.csv, triplets_by_weight.csv, forbidden_pairs.csv, genealogical_rules.json, topology_metrics.csv, skeleton2_metrics.csv to {}",
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

fn run_filtration_spec<I>(mut args: I) -> Result<(), String>
where
    I: Iterator<Item = String>,
{
    let mut spec_path: Option<PathBuf> = None;
    let mut jobs: Option<usize> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--spec" => {
                spec_path = Some(PathBuf::from(next_value(&mut args, "--spec")?));
            }
            "--jobs" => {
                let value = next_value(&mut args, "--jobs")?;
                let parsed = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --jobs value: {value}"))?;
                if parsed == 0 {
                    return Err("--jobs must be >= 1".to_string());
                }
                jobs = Some(parsed);
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
    let mut spec = load_filtration_spec(&path).map_err(|err| err.to_string())?;
    if let Some(value) = jobs {
        spec.jobs = Some(value);
    }
    let report = run_filtration(&spec).map_err(|err| err.to_string())?;
    write_filtration_summary(&report, &spec.out_dir).map_err(|err| err.to_string())?;
    println!(
        "wrote filtration_summary.csv and filtration_summary.md to {}",
        spec.out_dir.display()
    );
    Ok(())
}

#[derive(Default)]
struct CrossLoopCliConfig {
    spec_path: Option<PathBuf>,
    weight: Option<usize>,
    weight_min: Option<usize>,
    weight_max: Option<usize>,
    lower_weight: Option<usize>,
    loop_value: Option<usize>,
    weight_per_loop: usize,
    suffixes: Vec<Vec<String>>,
    suffix_toml_paths: Vec<PathBuf>,
    row_prefix: Option<Vec<String>>,
    row_limit: Option<usize>,
    residual_limit: usize,
    no_mapping: bool,
    export_constraints: bool,
    out_dir: Option<PathBuf>,
    prefactor_col: Option<usize>,
}

#[derive(Deserialize)]
struct SuffixesToml {
    suffixes: Vec<Vec<String>>,
}

struct EsymbRankScanCliConfig {
    data_dir: Option<PathBuf>,
    glob: Option<String>,
    loops: Vec<usize>,
    family_pow_last: bool,
    family_block2: bool,
    family_prefix: bool,
    family_suffix: bool,
    family_prefix_suffix: bool,
    x_set: Vec<String>,
    y_set: Vec<String>,
    pairs: Vec<String>,
    alphabet: Vec<String>,
    alphabet_project: bool,
    prefix_len: Option<usize>,
    suffix_len: Option<usize>,
    only_observed: bool,
    validate_marginals: bool,
    export_observables: bool,
    matrix_rank: bool,
    alphabet_mode: AlphabetMode,
    pairs_mode: PairsMode,
    r_budget: usize,
    primes: Vec<i64>,
    float_rank: bool,
    float_tau: f64,
    subsample_rank: bool,
    subsample_size: usize,
    seed: u64,
    plateau_len: usize,
    normalize: NormalizeChoice,
    skip_trivial: bool,
    attempt_solve_inconclusive: bool,
    out_dir: Option<PathBuf>,
}

impl Default for EsymbRankScanCliConfig {
    fn default() -> Self {
        Self {
            data_dir: None,
            glob: None,
            loops: Vec::new(),
            family_pow_last: false,
            family_block2: false,
            family_prefix: false,
            family_suffix: false,
            family_prefix_suffix: false,
            x_set: Vec::new(),
            y_set: Vec::new(),
            pairs: Vec::new(),
            alphabet: Vec::new(),
            alphabet_project: false,
            prefix_len: None,
            suffix_len: None,
            only_observed: false,
            validate_marginals: false,
            export_observables: false,
            matrix_rank: false,
            alphabet_mode: AlphabetMode::Manual,
            pairs_mode: PairsMode::Manual,
            r_budget: 0,
            primes: Vec::new(),
            float_rank: false,
            float_tau: 0.0,
            subsample_rank: false,
            subsample_size: 0,
            seed: 0,
            plateau_len: 0,
            normalize: NormalizeChoice::Auto,
            skip_trivial: true,
            attempt_solve_inconclusive: false,
            out_dir: None,
        }
    }
}

fn cross_loop(args: Vec<String>) -> Result<(), String> {
    let mut cfg = CrossLoopCliConfig {
        weight_per_loop: 2,
        residual_limit: 12,
        ..Default::default()
    };
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--spec" => {
                idx += 1;
                cfg.spec_path = Some(PathBuf::from(next_arg(&args, &mut idx, "--spec")?));
            }
            "--weight" => {
                idx += 1;
                cfg.weight = Some(parse_usize(
                    &next_arg(&args, &mut idx, "--weight")?,
                    "--weight",
                )?);
            }
            "--weight-min" => {
                idx += 1;
                cfg.weight_min = Some(parse_usize(
                    &next_arg(&args, &mut idx, "--weight-min")?,
                    "--weight-min",
                )?);
            }
            "--weight-max" => {
                idx += 1;
                cfg.weight_max = Some(parse_usize(
                    &next_arg(&args, &mut idx, "--weight-max")?,
                    "--weight-max",
                )?);
            }
            "--lower-weight" => {
                idx += 1;
                cfg.lower_weight = Some(parse_usize(
                    &next_arg(&args, &mut idx, "--lower-weight")?,
                    "--lower-weight",
                )?);
            }
            "--loop" | "--L" => {
                idx += 1;
                cfg.loop_value = Some(parse_usize(
                    &next_arg(&args, &mut idx, "--loop")?,
                    "--loop",
                )?);
            }
            "--weight-per-loop" => {
                idx += 1;
                cfg.weight_per_loop = parse_usize(
                    &next_arg(&args, &mut idx, "--weight-per-loop")?,
                    "--weight-per-loop",
                )?;
            }
            "--suffix" => {
                idx += 1;
                cfg.suffixes.push(parse_list(&args, &mut idx, "--suffix")?);
            }
            "--suffixes-toml" => {
                idx += 1;
                cfg.suffix_toml_paths.push(PathBuf::from(next_arg(
                    &args,
                    &mut idx,
                    "--suffixes-toml",
                )?));
            }
            "--row-prefix" => {
                idx += 1;
                cfg.row_prefix = Some(parse_list(&args, &mut idx, "--row-prefix")?);
            }
            "--row-limit" => {
                idx += 1;
                cfg.row_limit = Some(parse_usize(
                    &next_arg(&args, &mut idx, "--row-limit")?,
                    "--row-limit",
                )?);
            }
            "--residual-limit" => {
                idx += 1;
                cfg.residual_limit = parse_usize(
                    &next_arg(&args, &mut idx, "--residual-limit")?,
                    "--residual-limit",
                )?;
            }
            "--no-mapping" => {
                idx += 1;
                cfg.no_mapping = true;
            }
            "--export-constraints" => {
                idx += 1;
                cfg.export_constraints = true;
            }
            "--out" => {
                idx += 1;
                cfg.out_dir = Some(PathBuf::from(next_arg(&args, &mut idx, "--out")?));
            }
            "--prefactor-col" => {
                idx += 1;
                cfg.prefactor_col = Some(parse_usize(
                    &next_arg(&args, &mut idx, "--prefactor-col")?,
                    "--prefactor-col",
                )?);
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

    let spec_path = cfg
        .spec_path
        .ok_or_else(|| "missing --spec <path>".to_string())?;
    let exp_cfg = load_spec(&spec_path).map_err(|err| err.to_string())?;

    for path in &cfg.suffix_toml_paths {
        let mut loaded = load_suffixes_toml(path)?;
        cfg.suffixes.append(&mut loaded);
    }
    if cfg.suffixes.is_empty() {
        return Err("missing --suffix <letters...> or --suffixes-toml <path>".to_string());
    }
    let input_suffixes = cfg.suffixes.len();
    let mut seen_suffixes: BTreeSet<Vec<usize>> = BTreeSet::new();
    let mut suffix_specs = Vec::new();
    for names in &cfg.suffixes {
        let spec =
            SuffixSpec::from_names(&exp_cfg.alphabet, names).map_err(|err| err.to_string())?;
        if seen_suffixes.insert(spec.ids.clone()) {
            suffix_specs.push(spec);
        }
    }
    if suffix_specs.len() < input_suffixes {
        println!(
            "deduped suffix list: {} -> {}",
            input_suffixes,
            suffix_specs.len()
        );
    }

    let prefix_letters = match cfg.row_prefix.as_ref() {
        Some(names) => {
            Some(prefix_from_names(&exp_cfg.alphabet, names).map_err(|err| err.to_string())?)
        }
        None => None,
    };
    let row_filter = RowFilter {
        prefix: prefix_letters,
        max_rows: cfg.row_limit,
    };

    let scan_mode = cfg.weight_min.is_some() || cfg.weight_max.is_some();
    if scan_mode {
        if suffix_specs.is_empty() {
            return Err("missing --suffix <letters...>".to_string());
        }
        let weight_min = cfg
            .weight_min
            .ok_or_else(|| "missing --weight-min <n>".to_string())?;
        let weight_max = cfg
            .weight_max
            .ok_or_else(|| "missing --weight-max <n>".to_string())?;
        let base_out_dir = cfg
            .out_dir
            .unwrap_or_else(|| exp_cfg.out_dir.join("cross_loop_scan"));
        if suffix_specs.len() == 1 {
            let options = CrossLoopScanOptions {
                weight_min,
                weight_max,
                suffix: suffix_specs[0].clone(),
                suffix_index: 0,
                suffix_total: suffix_specs.len(),
                row_filter,
                residual_word_limit: cfg.residual_limit,
                compute_mapping: !cfg.no_mapping,
                prefactor_col: cfg.prefactor_col,
            };
            let report = run_cross_loop_scan(&exp_cfg, &options).map_err(|err| err.to_string())?;
            write_cross_loop_scan_outputs(&report, &base_out_dir).map_err(|err| err.to_string())?;
            println!(
                "wrote cross_loop_scan.csv and cross_loop_scan_fits.txt to {}",
                base_out_dir.display()
            );
            return Ok(());
        }

        fs::create_dir_all(&base_out_dir).map_err(|err| err.to_string())?;
        let mut used = std::collections::BTreeSet::new();
        let mut index_rows = Vec::new();

        let suffix_total = suffix_specs.len();
        for (suffix_index, suffix) in suffix_specs.into_iter().enumerate() {
            let label = unique_suffix_label(&suffix.names, &mut used);
            let out_dir = base_out_dir.join(&label);
            let options = CrossLoopScanOptions {
                weight_min,
                weight_max,
                suffix,
                suffix_index,
                suffix_total,
                row_filter: row_filter.clone(),
                residual_word_limit: cfg.residual_limit,
                compute_mapping: !cfg.no_mapping,
                prefactor_col: cfg.prefactor_col,
            };
            let report = run_cross_loop_scan(&exp_cfg, &options).map_err(|err| err.to_string())?;
            write_cross_loop_scan_outputs(&report, &out_dir).map_err(|err| err.to_string())?;
            index_rows.push((format_suffix_names(&options.suffix.names), label));
        }

        let index_csv = render_cross_loop_scan_index(&index_rows);
        fs::write(base_out_dir.join("cross_loop_scan_index.csv"), index_csv)
            .map_err(|err| err.to_string())?;
        println!(
            "wrote cross_loop_scan_index.csv and per-suffix outputs to {}",
            base_out_dir.display()
        );
        return Ok(());
    }

    let weight = if let Some(weight) = cfg.weight {
        weight
    } else if let Some(loop_value) = cfg.loop_value {
        loop_value
            .checked_mul(cfg.weight_per_loop)
            .ok_or_else(|| "loop * weight-per-loop overflow".to_string())?
    } else {
        return Err("missing --weight <n> (or --loop <n>)".to_string());
    };

    if cfg.export_constraints && cfg.no_mapping {
        return Err("--export-constraints requires mapping".to_string());
    }

    if suffix_specs.len() != 1 {
        return Err("single-weight mode requires exactly one --suffix".to_string());
    }
    let options = CrossLoopOptions {
        weight,
        lower_weight: cfg.lower_weight,
        suffix: suffix_specs[0].clone(),
        row_filter,
        residual_word_limit: cfg.residual_limit,
        compute_mapping: !cfg.no_mapping,
    };
    let report = run_cross_loop(&exp_cfg, &options).map_err(|err| err.to_string())?;
    let out_dir = cfg
        .out_dir
        .unwrap_or_else(|| exp_cfg.out_dir.join(format!("cross_loop_w{weight}")));
    write_cross_loop_outputs(&report, &out_dir, cfg.export_constraints)
        .map_err(|err| err.to_string())?;
    println!("wrote cross_loop_report.txt to {}", out_dir.display());
    Ok(())
}

fn esymb_rank_scan(args: Vec<String>) -> Result<(), String> {
    let mut cfg = EsymbRankScanCliConfig {
        family_pow_last: true,
        r_budget: 6,
        primes: vec![1000003, 1000033, 1000037],
        float_tau: 1e-12,
        subsample_size: 4,
        seed: 0,
        plateau_len: 2,
        normalize: NormalizeChoice::Auto,
        skip_trivial: true,
        ..Default::default()
    };
    let mut family_seen = false;
    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--data-dir" => {
                idx += 1;
                cfg.data_dir = Some(PathBuf::from(next_arg(&args, &mut idx, "--data-dir")?));
            }
            "--glob" => {
                idx += 1;
                cfg.glob = Some(next_arg(&args, &mut idx, "--glob")?);
            }
            "--loops" => {
                idx += 1;
                cfg.loops = parse_loop_list(&args, &mut idx, "--loops")?;
            }
            "--family" => {
                idx += 1;
                let value = next_arg(&args, &mut idx, "--family")?;
                if !family_seen {
                    cfg.family_pow_last = false;
                    cfg.family_block2 = false;
                    cfg.family_prefix = false;
                    cfg.family_suffix = false;
                    cfg.family_prefix_suffix = false;
                    family_seen = true;
                }
                match value.as_str() {
                    "pow-last" => cfg.family_pow_last = true,
                    "block2" => cfg.family_block2 = true,
                    "prefix" => cfg.family_prefix = true,
                    "suffix" => cfg.family_suffix = true,
                    "prefix-suffix" => cfg.family_prefix_suffix = true,
                    other => return Err(format!("unknown --family value: {other}")),
                }
            }
            "--alphabet" => {
                idx += 1;
                let value = next_arg(&args, &mut idx, "--alphabet")?;
                cfg.alphabet_mode = match value.as_str() {
                    "manual" => AlphabetMode::Manual,
                    "auto" => AlphabetMode::Auto,
                    other => return Err(format!("invalid --alphabet value: {other}")),
                };
            }
            "--letters" => {
                idx += 1;
                cfg.alphabet = parse_list(&args, &mut idx, "--letters")?;
            }
            "--alphabet-project" => {
                idx += 1;
                cfg.alphabet_project = true;
            }
            "--x-set" => {
                idx += 1;
                cfg.x_set = parse_list(&args, &mut idx, "--x-set")?;
            }
            "--y-set" => {
                idx += 1;
                cfg.y_set = parse_list(&args, &mut idx, "--y-set")?;
            }
            "--pairs" => {
                idx += 1;
                let list = parse_list(&args, &mut idx, "--pairs")?;
                if list.len() == 1 && list[0] == "auto" {
                    cfg.pairs_mode = PairsMode::Auto;
                    cfg.pairs.clear();
                } else if list.iter().any(|value| value == "auto") {
                    return Err(
                        "invalid --pairs value: cannot mix auto with explicit letters".to_string(),
                    );
                } else {
                    cfg.pairs_mode = PairsMode::Manual;
                    cfg.pairs = list;
                }
            }
            "--prefix-len" | "--r" => {
                idx += 1;
                let value =
                    parse_usize(&next_arg(&args, &mut idx, "--prefix-len")?, "--prefix-len")?;
                if let Some(existing) = cfg.prefix_len {
                    if existing != value {
                        return Err("conflicting --prefix-len values".to_string());
                    }
                }
                cfg.prefix_len = Some(value);
            }
            "--suffix-len" | "--k" => {
                idx += 1;
                let value =
                    parse_usize(&next_arg(&args, &mut idx, "--suffix-len")?, "--suffix-len")?;
                if let Some(existing) = cfg.suffix_len {
                    if existing != value {
                        return Err("conflicting --suffix-len values".to_string());
                    }
                }
                cfg.suffix_len = Some(value);
            }
            "--only-observed" => {
                idx += 1;
                cfg.only_observed = true;
            }
            "--validate-marginals" => {
                idx += 1;
                cfg.validate_marginals = true;
            }
            "--export-observables" => {
                idx += 1;
                cfg.export_observables = true;
            }
            "--matrix-rank" => {
                idx += 1;
                cfg.matrix_rank = true;
            }
            "--r-budget" => {
                idx += 1;
                cfg.r_budget =
                    parse_usize(&next_arg(&args, &mut idx, "--r-budget")?, "--r-budget")?;
            }
            "--primes" => {
                idx += 1;
                cfg.primes = parse_i64_list(&args, &mut idx, "--primes")?;
            }
            "--float-rank" => {
                idx += 1;
                cfg.float_rank = true;
            }
            "--float-tau" => {
                idx += 1;
                cfg.float_tau = next_arg(&args, &mut idx, "--float-tau")?
                    .parse::<f64>()
                    .map_err(|_| "invalid --float-tau value".to_string())?;
            }
            "--subsample-rank" => {
                idx += 1;
                cfg.subsample_rank = true;
            }
            "--subsample-size" => {
                idx += 1;
                cfg.subsample_size = parse_usize(
                    &next_arg(&args, &mut idx, "--subsample-size")?,
                    "--subsample-size",
                )?;
            }
            "--seed" => {
                idx += 1;
                cfg.seed = parse_u64(&next_arg(&args, &mut idx, "--seed")?, "--seed")?;
            }
            "--plateau-len" => {
                idx += 1;
                cfg.plateau_len = parse_usize(
                    &next_arg(&args, &mut idx, "--plateau-len")?,
                    "--plateau-len",
                )?;
            }
            "--normalize" => {
                idx += 1;
                let value = next_arg(&args, &mut idx, "--normalize")?;
                cfg.normalize = match value.as_str() {
                    "none" => NormalizeChoice::None,
                    "odd-double-factorial" => NormalizeChoice::OddDoubleFactorial,
                    "even-double-factorial" => NormalizeChoice::EvenDoubleFactorial,
                    "factorial" => NormalizeChoice::FactorialLm1,
                    "central-binomial" => NormalizeChoice::CentralBinomialLm1,
                    "auto" => NormalizeChoice::Auto,
                    other => return Err(format!("invalid --normalize value: {other}")),
                };
            }
            "--skip-trivial" => {
                idx += 1;
                cfg.skip_trivial = true;
            }
            "--no-skip-trivial" => {
                idx += 1;
                cfg.skip_trivial = false;
            }
            "--attempt-solve-inconclusive" => {
                idx += 1;
                cfg.attempt_solve_inconclusive = true;
            }
            "--out" => {
                idx += 1;
                cfg.out_dir = Some(PathBuf::from(next_arg(&args, &mut idx, "--out")?));
            }
            "--out-dir" => {
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

    if cfg.pairs_mode == PairsMode::Auto {
        cfg.alphabet_mode = AlphabetMode::Auto;
    }

    if cfg.loops.is_empty() {
        return Err("missing --loops <list or range>".to_string());
    }
    if cfg.data_dir.is_none() && cfg.glob.is_none() {
        return Err("missing --data-dir <path> or --glob <pattern>".to_string());
    }

    let out_dir = cfg
        .out_dir
        .unwrap_or_else(|| PathBuf::from("reports/esymb_rank_scan"));
    let config = EsymbRankScanConfig {
        data_dir: cfg.data_dir,
        glob: cfg.glob,
        loops: cfg.loops,
        family_pow_last: cfg.family_pow_last,
        family_block2: cfg.family_block2,
        family_prefix: cfg.family_prefix,
        family_suffix: cfg.family_suffix,
        family_prefix_suffix: cfg.family_prefix_suffix,
        x_set: cfg.x_set,
        y_set: cfg.y_set,
        pairs: cfg.pairs,
        alphabet: cfg.alphabet,
        alphabet_project: cfg.alphabet_project,
        prefix_len: cfg.prefix_len,
        suffix_len: cfg.suffix_len,
        only_observed: cfg.only_observed,
        validate_marginals: cfg.validate_marginals,
        export_observables: cfg.export_observables,
        matrix_rank: cfg.matrix_rank,
        r_budget: cfg.r_budget,
        primes: cfg.primes,
        float_rank: cfg.float_rank,
        float_tau: cfg.float_tau,
        subsample_rank: cfg.subsample_rank,
        subsample_size: cfg.subsample_size,
        seed: cfg.seed,
        plateau_len: cfg.plateau_len,
        normalize: cfg.normalize,
        skip_trivial: cfg.skip_trivial,
        alphabet_mode: cfg.alphabet_mode,
        pairs_mode: cfg.pairs_mode,
        attempt_solve_inconclusive: cfg.attempt_solve_inconclusive,
        out_dir: out_dir.clone(),
    };

    let report = run_esymb_rank_scan(&config).map_err(|err| err.to_string())?;
    println!(
        "esymb-rank-scan loops={:?} sequences={} out={}",
        report.loops,
        report.sequences.len(),
        out_dir.display()
    );
    for meta in &report.loop_meta {
        println!(
            "L{} merged_terms={} source={}",
            meta.loop_index,
            meta.merged_terms,
            meta.source.display()
        );
    }
    Ok(())
}

fn esymb_span_deps(args: Vec<String>) -> Result<(), String> {
    let mut input: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut family = SpanFamilyFilter::All;
    let mut support_max = 3usize;
    let mut coef_set = CoefSet::Pm1;
    let mut top_k = 200usize;
    let mut export_forbidden = false;
    let mut export_equiv_classes = false;

    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--in" | "--observables" => {
                idx += 1;
                let value = PathBuf::from(next_arg(&args, &mut idx, "--in")?);
                if let Some(existing) = &input {
                    if existing != &value {
                        return Err("conflicting --in/--observables values".to_string());
                    }
                } else {
                    input = Some(value);
                }
            }
            "--out-dir" | "--out" => {
                idx += 1;
                out_dir = Some(PathBuf::from(next_arg(&args, &mut idx, "--out-dir")?));
            }
            "--family" => {
                idx += 1;
                let value = next_arg(&args, &mut idx, "--family")?;
                family = match value.as_str() {
                    "all" => SpanFamilyFilter::All,
                    "prefix" => SpanFamilyFilter::Prefix,
                    "suffix" => SpanFamilyFilter::Suffix,
                    "prefix-suffix" => SpanFamilyFilter::PrefixSuffix,
                    other => return Err(format!("invalid --family value: {other}")),
                };
            }
            "--support-max" => {
                idx += 1;
                support_max = parse_usize(
                    &next_arg(&args, &mut idx, "--support-max")?,
                    "--support-max",
                )?;
            }
            "--coef-set" => {
                idx += 1;
                let value = next_arg(&args, &mut idx, "--coef-set")?;
                coef_set = match value.as_str() {
                    "pm1" => CoefSet::Pm1,
                    "pm2" => CoefSet::Pm2,
                    other => return Err(format!("invalid --coef-set value: {other}")),
                };
            }
            "--top-k" => {
                idx += 1;
                top_k = parse_usize(&next_arg(&args, &mut idx, "--top-k")?, "--top-k")?;
            }
            "--export-forbidden" => {
                idx += 1;
                export_forbidden = true;
            }
            "--export-equiv-classes" => {
                idx += 1;
                export_equiv_classes = true;
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

    let input = input.ok_or_else(|| "missing --observables <path>".to_string())?;
    let out_dir = out_dir.unwrap_or_else(|| PathBuf::from("reports/esymb_span_deps"));
    let cfg = EsymbSpanDepsConfig {
        input,
        out_dir: out_dir.clone(),
        family,
        support_max,
        coef_set,
        top_k,
        export_forbidden,
        export_equiv_classes,
    };
    let report = run_esymb_span_deps(&cfg).map_err(|err| err.to_string())?;
    println!(
        "esymb-span-deps loops={:?} observables={} trivial={} out={}",
        report.loops,
        report.total_observables,
        report.trivial_observables,
        out_dir.display()
    );
    Ok(())
}

fn esymb_hankel_subblock(args: Vec<String>) -> Result<(), String> {
    let mut input: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut r: Option<usize> = None;
    let mut k: Option<usize> = None;
    let mut loops: Option<Vec<usize>> = None;
    let mut primes = vec![1000003, 1000033, 1000037];
    let mut exact = false;

    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--in" => {
                idx += 1;
                input = Some(PathBuf::from(next_arg(&args, &mut idx, "--in")?));
            }
            "--out-dir" | "--out" => {
                idx += 1;
                out_dir = Some(PathBuf::from(next_arg(&args, &mut idx, "--out-dir")?));
            }
            "--r" => {
                idx += 1;
                r = Some(parse_usize(&next_arg(&args, &mut idx, "--r")?, "--r")?);
            }
            "--k" => {
                idx += 1;
                k = Some(parse_usize(&next_arg(&args, &mut idx, "--k")?, "--k")?);
            }
            "--loops" => {
                idx += 1;
                loops = Some(parse_loop_list(&args, &mut idx, "--loops")?);
            }
            "--primes" => {
                idx += 1;
                primes = parse_i64_list(&args, &mut idx, "--primes")?;
            }
            "--exact" => {
                idx += 1;
                exact = true;
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

    let input = input.ok_or_else(|| "missing --in <path>".to_string())?;
    let r = r.ok_or_else(|| "missing --r <n>".to_string())?;
    let k = k.ok_or_else(|| "missing --k <n>".to_string())?;
    let out_dir = out_dir.unwrap_or_else(|| PathBuf::from("reports/esymb_hankel_subblock"));

    let cfg = EsymbHankelSubblockConfig {
        input,
        out_dir: out_dir.clone(),
        r,
        k,
        loops,
        primes,
        exact,
    };
    let report = run_esymb_hankel_subblock(&cfg).map_err(|err| err.to_string())?;
    println!(
        "esymb-hankel-subblock loops={:?} rows={} cols={} out={}",
        report.loops,
        report.nrows,
        report.ncols,
        out_dir.display()
    );
    Ok(())
}

fn path1_toy(args: Vec<String>) -> Result<(), String> {
    let mut mode = Path1Mode::Oracle;
    let mut out_dir: Option<PathBuf> = None;
    let mut weights: Option<Vec<usize>> = None;
    let mut loops: Option<Vec<usize>> = None;
    let mut max_words: u64 = 40_000;
    let mut max_alternations = 3usize;
    let mut max_terms: Option<usize> = None;
    let mut export_oracle_jsonl = false;
    let mut run_esymb = false;

    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--mode" => {
                idx += 1;
                let value = next_arg(&args, &mut idx, "--mode")?;
                mode = match value.as_str() {
                    "oracle" => Path1Mode::Oracle,
                    "scaled" => Path1Mode::Scaled,
                    other => return Err(format!("invalid --mode value: {other}")),
                };
            }
            "--out-dir" | "--out" => {
                idx += 1;
                out_dir = Some(PathBuf::from(next_arg(&args, &mut idx, "--out-dir")?));
            }
            "--weights" => {
                idx += 1;
                weights = Some(parse_loop_list(&args, &mut idx, "--weights")?);
            }
            "--loops" => {
                idx += 1;
                loops = Some(parse_loop_list(&args, &mut idx, "--loops")?);
            }
            "--max-words" => {
                idx += 1;
                max_words = parse_u64(&next_arg(&args, &mut idx, "--max-words")?, "--max-words")?;
            }
            "--max-alternations" => {
                idx += 1;
                max_alternations = parse_usize(
                    &next_arg(&args, &mut idx, "--max-alternations")?,
                    "--max-alternations",
                )?;
            }
            "--max-terms" => {
                idx += 1;
                max_terms = Some(parse_usize(
                    &next_arg(&args, &mut idx, "--max-terms")?,
                    "--max-terms",
                )?);
            }
            "--export-jsonl" => {
                idx += 1;
                export_oracle_jsonl = true;
            }
            "--run-esymb" => {
                idx += 1;
                run_esymb = true;
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            other => return Err(format!("unknown arg: {other}")),
        }
    }

    let out_dir = out_dir.unwrap_or_else(|| PathBuf::from("reports/path1_toy"));
    let weights = weights.unwrap_or_else(|| (1..=12).collect());
    let loops = loops.unwrap_or_else(|| (1..=24).collect());

    let cfg = Path1ToyConfig {
        mode,
        out_dir: out_dir.clone(),
        weights,
        loops,
        max_words,
        max_alternations,
        max_terms,
        export_oracle_jsonl,
        run_esymb,
    };

    let report = run_path1_toy(&cfg).map_err(|err| err.to_string())?;
    println!(
        "path1-toy mode={} out={}",
        report.mode.as_str(),
        report.out_dir.display()
    );
    match report.mode {
        Path1Mode::Oracle => println!("weights={:?}", report.weights),
        Path1Mode::Scaled => println!(
            "loops={:?} esymb_pipeline={}",
            report.loops, report.ran_esymb
        ),
    }
    Ok(())
}

fn gen_ladder(args: Vec<String>) -> Result<(), String> {
    let mut out_dir: Option<PathBuf> = None;
    let mut loops: Option<Vec<usize>> = None;
    let mut prefix_len = 2usize;
    let mut suffix_len = 2usize;
    let mut family = LadderFamily::PrefixSuffix;
    let mut emit_jsonl = false;
    let mut max_terms: u64 = 5_000_000;
    let mut data_dir: Option<PathBuf> = None;
    let mut validate = true;
    let mut matrix_rank = false;
    let mut reference_bruteforce_max_loop = 5usize;

    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--out-dir" | "--out" => {
                idx += 1;
                out_dir = Some(PathBuf::from(next_arg(&args, &mut idx, "--out-dir")?));
            }
            "--loops" => {
                idx += 1;
                loops = Some(parse_loop_list(&args, &mut idx, "--loops")?);
            }
            "--prefix-len" | "--r" => {
                idx += 1;
                prefix_len =
                    parse_usize(&next_arg(&args, &mut idx, "--prefix-len")?, "--prefix-len")?;
            }
            "--suffix-len" | "--k" => {
                idx += 1;
                suffix_len =
                    parse_usize(&next_arg(&args, &mut idx, "--suffix-len")?, "--suffix-len")?;
            }
            "--family" => {
                idx += 1;
                let value = next_arg(&args, &mut idx, "--family")?;
                family = LadderFamily::parse(&value)
                    .ok_or_else(|| format!("unknown --family value: {value}"))?;
            }
            "--emit-jsonl" => {
                idx += 1;
                emit_jsonl = true;
            }
            "--max-terms" => {
                idx += 1;
                max_terms = parse_u64(&next_arg(&args, &mut idx, "--max-terms")?, "--max-terms")?;
            }
            "--data-dir" => {
                idx += 1;
                data_dir = Some(PathBuf::from(next_arg(&args, &mut idx, "--data-dir")?));
            }
            "--validate" => {
                idx += 1;
                validate = true;
            }
            "--no-validate" => {
                idx += 1;
                validate = false;
            }
            "--matrix-rank" => {
                idx += 1;
                matrix_rank = true;
            }
            "--reference-bruteforce-max-loop" => {
                idx += 1;
                reference_bruteforce_max_loop = parse_usize(
                    &next_arg(&args, &mut idx, "--reference-bruteforce-max-loop")?,
                    "--reference-bruteforce-max-loop",
                )?;
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

    let out_dir = out_dir.ok_or_else(|| "missing --out-dir <path>".to_string())?;
    let loops = loops.ok_or_else(|| "missing --loops <list>".to_string())?;

    let cfg = LadderGenConfig {
        out_dir: out_dir.clone(),
        loops,
        prefix_len,
        suffix_len,
        family,
        emit_jsonl,
        max_terms,
        data_dir,
        validate,
        matrix_rank,
        reference_bruteforce_max_loop,
    };
    let report = run_ladder_gen(&cfg).map_err(|err| err.to_string())?;
    println!(
        "gen-ladder family={} loops={:?} out={}",
        cfg.family.as_str(),
        report.loops,
        report.out_dir.display()
    );
    if let Some(dir) = report.data_dir {
        println!("jsonl_dir={}", dir.display());
    }
    Ok(())
}

fn gen_pentaladder(args: Vec<String>) -> Result<(), String> {
    let mut out_dir: Option<PathBuf> = None;
    let mut loops: Option<Vec<usize>> = None;
    let mut prefix_len = 2usize;
    let mut suffix_len = 2usize;
    let mut family = PentaladderFamily::PrefixSuffix;
    let mut stats_only = false;
    let mut emit_jsonl = false;
    let mut max_terms: u64 = 5_000_000;
    let mut data_dir: Option<PathBuf> = None;
    let mut validate = true;
    let mut validate_integrability_max_loop = 4usize;
    let mut matrix_rank = false;

    let mut idx = 0usize;
    while idx < args.len() {
        match args[idx].as_str() {
            "--out-dir" | "--out" => {
                idx += 1;
                out_dir = Some(PathBuf::from(next_arg(&args, &mut idx, "--out-dir")?));
            }
            "--loops" => {
                idx += 1;
                loops = Some(parse_loop_list(&args, &mut idx, "--loops")?);
            }
            "--prefix-len" | "--r" => {
                idx += 1;
                prefix_len =
                    parse_usize(&next_arg(&args, &mut idx, "--prefix-len")?, "--prefix-len")?;
            }
            "--suffix-len" | "--k" => {
                idx += 1;
                suffix_len =
                    parse_usize(&next_arg(&args, &mut idx, "--suffix-len")?, "--suffix-len")?;
            }
            "--family" => {
                idx += 1;
                let value = next_arg(&args, &mut idx, "--family")?;
                family = PentaladderFamily::parse(&value)
                    .ok_or_else(|| format!("unknown --family value: {value}"))?;
            }
            "--stats-only" => {
                idx += 1;
                stats_only = true;
            }
            "--emit-jsonl" => {
                idx += 1;
                emit_jsonl = true;
            }
            "--max-terms" => {
                idx += 1;
                max_terms = parse_u64(&next_arg(&args, &mut idx, "--max-terms")?, "--max-terms")?;
            }
            "--data-dir" => {
                idx += 1;
                data_dir = Some(PathBuf::from(next_arg(&args, &mut idx, "--data-dir")?));
            }
            "--matrix-rank" => {
                idx += 1;
                matrix_rank = true;
            }
            "--validate" => {
                idx += 1;
                validate = true;
            }
            "--no-validate" => {
                idx += 1;
                validate = false;
            }
            "--validate-integrability-max-loop" => {
                idx += 1;
                validate_integrability_max_loop = parse_usize(
                    &next_arg(&args, &mut idx, "--validate-integrability-max-loop")?,
                    "--validate-integrability-max-loop",
                )?;
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

    let out_dir = out_dir.ok_or_else(|| "missing --out-dir <path>".to_string())?;
    let loops = loops.ok_or_else(|| "missing --loops <list>".to_string())?;

    let cfg = PentaladderGenConfig {
        out_dir: out_dir.clone(),
        loops,
        prefix_len,
        suffix_len,
        family,
        stats_only,
        emit_jsonl,
        max_terms,
        data_dir,
        validate,
        validate_integrability_max_loop,
        matrix_rank,
    };
    let report = run_pentaladder_gen(&cfg).map_err(|err| err.to_string())?;
    println!(
        "gen-pentaladder family={} loops={:?} out={}",
        cfg.family.as_str(),
        report.loops,
        report.out_dir.display()
    );
    if let Some(dir) = report.data_dir {
        println!("jsonl_dir={}", dir.display());
    }
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
    println!("mpl-experiments (M1 runner + cross-loop)");
    println!();
    println!("Usage:");
    println!("  mpl-experiments run --spec <path>");
    println!("  mpl-experiments count --spec <path>");
    println!("  mpl-experiments filtration --spec <path> [--jobs N]");
    println!(
        "  mpl-experiments cross-loop --spec <path> --weight <n> --suffix <letters...> [options]"
    );
    println!("  mpl-experiments cross-loop --spec <path> --weight-min <n> --weight-max <n> --suffix <letters...> [options]");
    println!("  mpl-experiments esymb-rank-scan --data-dir <dir> --loops <list> [options]");
    println!("  mpl-experiments esymb-span-deps --observables <path> [options]");
    println!("  mpl-experiments esymb-hankel-subblock --in <path> --r <n> --k <n> [options]");
    println!("  mpl-experiments path1-toy --mode <oracle|scaled> [options]");
    println!("  mpl-experiments gen-ladder --out-dir <dir> --loops <list> [options]");
    println!("  mpl-experiments gen-pentaladder --out-dir <dir> --loops <list> [options]");
    println!("  mpl-experiments kze2-hankel-mvp [options]");
    println!();
    println!("Options:");
    println!("  --spec <path>          Experiment TOML spec");
    println!("  --jobs <n>             Filtration worker count");
    println!("  --weight <n>           Upper weight for cross-loop");
    println!("  --lower-weight <n>     Lower weight override for cross-loop");
    println!("  --loop <n>             Loop label (weight defaults to loop * weight-per-loop)");
    println!("  --weight-per-loop <n>  Weight multiplier for --loop (default: 2)");
    println!("  --weight-min <n>       Scan range start weight");
    println!("  --weight-max <n>       Scan range end weight");
    println!("  --suffix <letters...>  Suffix letters (repeatable in scan mode)");
    println!("  --suffixes-toml <path> Load suffix list from TOML (repeatable)");
    println!("  --row-prefix <letters...>  Prefix filter for truncated words");
    println!("  --row-limit <n>        Row cap for rank calculations");
    println!("  --residual-limit <n>   Residual sample word cap (default: 12)");
    println!("  --no-mapping           Skip mapping into lower space");
    println!("  --export-constraints   Export coupled constraints (requires mapping)");
    println!("  --prefactor-col <n>    Column index for prefactor extraction (scan mode)");
    println!("  --out <dir>            Output directory override");
    println!("  --out-dir <dir>        Output directory override (alias for --out)");
    println!("  --data-dir <dir>       Directory with Esymb_L*.jsonl (esymb-rank-scan)");
    println!("  --glob <pattern>       Glob for Esymb_L*.jsonl (esymb-rank-scan)");
    println!("  --loops <list>         Loop list or range (e.g. 1..6,3,5)");
    println!("  --family <name>        pow-last|block2|prefix|suffix|prefix-suffix (repeatable)");
    println!("  --alphabet <mode>      manual|auto (default: manual)");
    println!("  --letters <letters...> Manual alphabet list for prefix/suffix families");
    println!("  --alphabet-project     Drop terms with letters outside alphabet (prefix/suffix)");
    println!("  --prefix-len <n>       Prefix length for prefix/prefix-suffix families");
    println!("  --suffix-len <n>       Suffix length for suffix/prefix-suffix families");
    println!("  --r <n>                Alias for --prefix-len");
    println!("  --k <n>                Alias for --suffix-len");
    println!("  --only-observed        Emit only observed prefix/suffix buckets");
    println!("  --validate-marginals   Check prefix/suffix conservation identities");
    println!("  --export-observables   Write marginals_observables.csv");
    println!("  --matrix-rank          Write marginals_matrix_rank.csv");
    println!("  --x-set <letters...>   Letter set for pow-last family");
    println!("  --y-set <letters...>   Letter set for pow-last family");
    println!("  --pairs <letters...>   Letter set for block2 family (or auto)");
    println!("  --r-budget <n>         Max rank order to solve (default: 6)");
    println!(
        "  --primes <list>        Prime list for mod-p rank (default: 1000003,1000033,1000037)"
    );
    println!("  --float-rank           Enable float rank curve");
    println!("  --float-tau <tau>      Float rank tolerance (default: 1e-12)");
    println!("  --subsample-rank       Enable subsample rank curve");
    println!("  --subsample-size <n>   Subsample size (default: 4)");
    println!("  --seed <n>             RNG seed (default: 0)");
    println!("  --plateau-len <n>      Plateau window length (default: 2)");
    println!("  --normalize <mode>     none|odd-double-factorial|even-double-factorial|factorial|central-binomial|auto (default: auto)");
    println!("  --skip-trivial         Skip trivial all-zero sequences (default: true)");
    println!("  --no-skip-trivial      Disable trivial-sequence skipping");
    println!("  --attempt-solve-inconclusive  Try exact solve on inconclusive sequences");
    println!("  --observables <path>  Input marginals_observables.csv (esymb-span-deps)");
    println!("  --in <path>           Alias for --observables");
    println!("  --family <name>       prefix|suffix|prefix-suffix|all (esymb-span-deps)");
    println!("  --support-max <n>     Max support size (default: 3)");
    println!("  --coef-set <name>     pm1|pm2 coefficient set (default: pm1)");
    println!("  --top-k <n>           Limit sparse relations (default: 200)");
    println!("  --export-forbidden    Write forbidden_keys.csv and nonzero_keys.csv");
    println!("  --export-equiv-classes  Write equiv_classes.csv");
    println!("  --in <path>           Input marginals_observables.csv (esymb-hankel-subblock)");
    println!("  --r <n>               Prefix length (esymb-hankel-subblock)");
    println!("  --k <n>               Suffix length (esymb-hankel-subblock)");
    println!("  --loops <list>        Loop list or range (esymb-hankel-subblock)");
    println!("  --primes <list>       Prime list for mod-p rank (esymb-hankel-subblock)");
    println!("  --exact               Enable mod-p row/col dependencies (max-rank prime)");
    println!("  --mode <name>         oracle|scaled (path1-toy)");
    println!("  --weights <list>      Weight list/range (path1-toy oracle, default: 1..12)");
    println!("  --loops <list>        Loop list/range (path1-toy scaled, default: 1..24)");
    println!("  --max-words <n>       Word cap for path1-toy (default: 40000)");
    println!("  --max-alternations <n>  Max alternations (path1-toy scaled, default: 3)");
    println!("  --max-terms <n>       Term cap for synthesized symbols (path1-toy)");
    println!("  --export-jsonl        Write oracle symbols_jsonl (path1-toy)");
    println!("  --run-esymb           Run ESymb pipeline (path1-toy scaled)");
    println!("  --family <name>       prefix|suffix|prefix-suffix (gen-ladder)");
    println!("  --prefix-len <n>      Prefix length (gen-ladder)");
    println!("  --suffix-len <n>      Suffix length (gen-ladder)");
    println!("  --emit-jsonl          Write Esymb_L*.jsonl (gen-ladder)");
    println!(
        "  --data-dir <dir>      JSONL output dir (gen-ladder, default: out_dir/converted_jsonl)"
    );
    println!("  --max-terms <n>       Term cap for JSONL emission (gen-ladder)");
    println!("  --matrix-rank         Write marginals_matrix_rank.csv (gen-ladder)");
    println!("  --validate            Enable DE/integrability checks (gen-ladder)");
    println!("  --no-validate         Disable DE/integrability checks (gen-ladder)");
    println!("  --reference-bruteforce-max-loop <n>  Max loop for validation (gen-ladder)");
    println!("  --family <name>       prefix|suffix|prefix-suffix (gen-pentaladder)");
    println!("  --prefix-len <n>      Prefix length (gen-pentaladder)");
    println!("  --suffix-len <n>      Suffix length (gen-pentaladder)");
    println!("  --stats-only          Stream-only marginals (gen-pentaladder)");
    println!("  --emit-jsonl          Write Esymb_L*.jsonl (gen-pentaladder)");
    println!("  --data-dir <dir>      JSONL output dir (gen-pentaladder, default: out_dir/converted_jsonl)");
    println!("  --max-terms <n>       Term cap for JSONL emission (gen-pentaladder)");
    println!("  --matrix-rank         Write marginals_matrix_rank.csv (gen-pentaladder)");
    println!("  --validate            Enable validation checks (gen-pentaladder)");
    println!("  --no-validate         Disable validation checks (gen-pentaladder)");
    println!("  --validate-integrability-max-loop <n>  Max loop for integrability check (gen-pentaladder)");
    println!("  --prime <p>          Prime modulus (kze2-hankel-mvp)");
    println!("  --prefix-len <n>     Prefix/suffix length L (kze2-hankel-mvp)");
    println!("  --holdout-len <n>    Holdout length (kze2-hankel-mvp)");
    println!("  --r <n>              Even dimension (kze2-hankel-mvp)");
    println!("  --out-dir <dir>      Output directory (kze2-hankel-mvp)");
    println!("  --help                 Show this help");
}

fn unique_suffix_label(names: &[String], used: &mut std::collections::BTreeSet<String>) -> String {
    let base = suffix_label(names);
    if !used.contains(&base) {
        used.insert(base.clone());
        return base;
    }
    let mut counter = 1usize;
    loop {
        let candidate = format!("{base}_{counter}");
        if !used.contains(&candidate) {
            used.insert(candidate.clone());
            return candidate;
        }
        counter = counter.saturating_add(1);
    }
}

fn suffix_label(names: &[String]) -> String {
    let joined = names.join("_");
    let mut out = String::new();
    let mut prev_underscore = false;
    for ch in joined.chars() {
        let mapped = if ch.is_ascii_alphanumeric() { ch } else { '_' };
        if mapped == '_' {
            if prev_underscore {
                continue;
            }
            prev_underscore = true;
        } else {
            prev_underscore = false;
        }
        out.push(mapped);
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "suffix".to_string()
    } else {
        trimmed.to_string()
    }
}

fn format_suffix_names(names: &[String]) -> String {
    names.join(" ")
}

fn load_suffixes_toml(path: &PathBuf) -> Result<Vec<Vec<String>>, String> {
    let contents = fs::read_to_string(path)
        .map_err(|err| format!("failed to read suffixes TOML {}: {err}", path.display()))?;
    let parsed: SuffixesToml = toml::from_str(&contents)
        .map_err(|err| format!("failed to parse suffixes TOML {}: {err}", path.display()))?;
    if parsed.suffixes.is_empty() {
        return Err(format!(
            "suffixes TOML {} must include at least one suffix",
            path.display()
        ));
    }
    for (idx, suffix) in parsed.suffixes.iter().enumerate() {
        if suffix.is_empty() {
            return Err(format!(
                "suffixes TOML {} has empty suffix at index {idx}",
                path.display()
            ));
        }
    }
    Ok(parsed.suffixes)
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

fn parse_i64_list(args: &[String], idx: &mut usize, flag: &str) -> Result<Vec<i64>, String> {
    let values = parse_list(args, idx, flag)?;
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        let parsed = value
            .parse::<i64>()
            .map_err(|_| format!("invalid {flag} value: {value}"))?;
        out.push(parsed);
    }
    if out.is_empty() {
        return Err(format!("missing values after {flag}"));
    }
    Ok(out)
}

fn parse_loop_list(args: &[String], idx: &mut usize, flag: &str) -> Result<Vec<usize>, String> {
    let mut out = Vec::new();
    while *idx < args.len() {
        let value = &args[*idx];
        if value.starts_with("--") {
            break;
        }
        for part in value.split(',') {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some((start, end)) = trimmed.split_once("..") {
                let start_val = start
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| format!("invalid {flag} range start: {trimmed}"))?;
                let end_val = end
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| format!("invalid {flag} range end: {trimmed}"))?;
                if start_val > end_val {
                    return Err(format!("invalid {flag} range: {trimmed}"));
                }
                for value in start_val..=end_val {
                    out.push(value);
                }
            } else {
                let value = trimmed
                    .parse::<usize>()
                    .map_err(|_| format!("invalid {flag} value: {trimmed}"))?;
                out.push(value);
            }
        }
        *idx += 1;
    }
    if out.is_empty() {
        return Err(format!("missing values after {flag}"));
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

fn parse_list(args: &[String], idx: &mut usize, flag: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    while *idx < args.len() {
        let value = &args[*idx];
        if value.starts_with("--") {
            break;
        }
        for part in value.split(',') {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
        }
        *idx += 1;
    }
    if out.is_empty() {
        return Err(format!("missing values after {flag}"));
    }
    Ok(out)
}
