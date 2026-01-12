use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use mpl_symbol::space::{
    build_word_count_cache, Alphabet, ChannelPairsAcceptor, ConstraintBudget, GenealogicalAcceptor,
    KGramAcceptor, SampleTable, WordConstraints,
};
use mpl_symbol::SymbolError;
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;

use crate::build::acceptors::{AutomatonAcceptorRef, CompositeAcceptor};
use crate::build::alphabet::collect_vars_from_letters;
use crate::output::single::{write_count_only, write_outputs};
use crate::run::count::{run_count_only, CountReport, CountSummary};
use crate::run::single::{
    convert_word_count, error_code_from_symbol, run_experiment, run_experiment_with_counts,
    ExperimentConfig,
};
use crate::util::sanitize::sanitize_layer_name;
use crate::util::signature::{signature_from_count_report, signature_from_full_report};
use crate::{ErrorCode, ExperimentError, Status};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FiltrationMode {
    Full,
    CountOnly,
    Auto,
}

impl FiltrationMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::CountOnly => "count_only",
            Self::Auto => "auto",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FiltrationLayer {
    pub name: String,
    pub mode: FiltrationMode,
    pub constraints: WordConstraints,
    pub genealogical_acceptors: Vec<GenealogicalAcceptor>,
    pub kgram_acceptors: Vec<KGramAcceptor>,
    pub channel_pairs_acceptors: Vec<ChannelPairsAcceptor>,
    pub automaton_acceptors: Vec<AutomatonAcceptorRef>,
    pub constraint_budget: ConstraintBudget,
}

#[derive(Clone, Debug)]
pub struct FiltrationSpec {
    pub name: String,
    pub out_dir: PathBuf,
    pub alphabet: Alphabet,
    pub weight_min: usize,
    pub weight_max: usize,
    pub vars: Vec<String>,
    pub repeats: usize,
    pub full_run_max_words: Option<u64>,
    pub jobs: Option<usize>,
    pub sample_table: SampleTable,
    pub layers: Vec<FiltrationLayer>,
}

#[derive(Clone, Debug)]
pub struct FiltrationLayerInfo {
    pub index: usize,
    pub name: String,
    pub mode: FiltrationMode,
    pub dir_name: String,
}

#[derive(Clone, Debug)]
pub struct FiltrationSummaryRow {
    pub layer_index: usize,
    pub layer_name: String,
    pub weight: usize,
    pub mode: FiltrationMode,
    pub status: Status,
    pub error_code: Option<ErrorCode>,
    pub n_words_allowed: usize,
    pub dim: Option<usize>,
    pub rank: Option<usize>,
    pub basis_ncols: Option<usize>,
    pub rows_attempted: Option<u64>,
    pub rows_inserted: Option<u64>,
    pub samples_used: Option<u64>,
    pub envs_total: Option<u64>,
    pub constraints_insufficient_samples: Option<u64>,
    pub sample_table: SampleTable,
}

#[derive(Clone, Debug)]
pub struct FiltrationReport {
    pub name: String,
    pub out_dir: PathBuf,
    pub weight_min: usize,
    pub weight_max: usize,
    pub layers: Vec<FiltrationLayerInfo>,
    pub rows: Vec<FiltrationSummaryRow>,
}

#[derive(Clone, Debug)]
struct FiltrationTask {
    index: usize,
    layer_index: usize,
    weight: usize,
    weight_idx: usize,
    layer_dir: PathBuf,
    count_summary: CountSummary,
    precounts: Option<Arc<Vec<Result<usize, SymbolError>>>>,
    spec: Arc<FiltrationSpec>,
    vars: Arc<Vec<String>>,
}

pub fn run_filtration(spec: &FiltrationSpec) -> Result<FiltrationReport, ExperimentError> {
    if spec.weight_min > spec.weight_max {
        return Err(ExperimentError::InvalidConfig(
            "weight_min must be <= weight_max".to_string(),
        ));
    }
    if spec.repeats == 0 {
        return Err(ExperimentError::InvalidConfig(
            "repeats must be >= 1".to_string(),
        ));
    }
    if let Some(jobs) = spec.jobs {
        if jobs == 0 {
            return Err(ExperimentError::InvalidConfig(
                "jobs must be >= 1".to_string(),
            ));
        }
    }

    let vars = if spec.vars.is_empty() {
        collect_vars_from_letters(&spec.alphabet.letters)
    } else {
        spec.vars.clone()
    };

    fs::create_dir_all(&spec.out_dir)?;
    let layers_dir = spec.out_dir.join("layers");
    fs::create_dir_all(&layers_dir)?;

    let mut layers = Vec::with_capacity(spec.layers.len());
    let mut tasks = Vec::new();
    let alpha_len = spec.alphabet.letters.len();
    let spec = Arc::new(spec.clone());
    let vars = Arc::new(vars);

    for (layer_index, layer) in spec.layers.iter().enumerate() {
        let sanitized = sanitize_layer_name(&layer.name);
        let dir_name = format!("{layer_index}_{sanitized}");
        let layer_dir = layers_dir.join(&dir_name);
        fs::create_dir_all(&layer_dir)?;

        layers.push(FiltrationLayerInfo {
            index: layer_index,
            name: layer.name.clone(),
            mode: layer.mode,
            dir_name: dir_name.clone(),
        });

        let range_len = spec.weight_max.saturating_sub(spec.weight_min) + 1;
        let acceptor = CompositeAcceptor::new(
            &layer.constraints,
            &layer.automaton_acceptors,
            &layer.kgram_acceptors,
            &layer.genealogical_acceptors,
            &layer.channel_pairs_acceptors,
        );
        let budget = layer.constraint_budget;

        let (precounts, count_summaries) =
            match build_word_count_cache(alpha_len, &acceptor, spec.weight_max, budget) {
                Ok(mut cache) => {
                    let results = cache.count_range(spec.weight_min, spec.weight_max);
                    let mut precounts = Vec::with_capacity(range_len);
                    let mut summaries = Vec::with_capacity(range_len);
                    for (weight, result) in (spec.weight_min..=spec.weight_max).zip(results) {
                        match result.and_then(convert_word_count) {
                            Ok(count) => {
                                precounts.push(Ok(count));
                                summaries.push(CountSummary {
                                    weight,
                                    n_words_allowed: count,
                                    status: Status::Ok,
                                    error_code: None,
                                });
                            }
                            Err(err) => {
                                let code = error_code_from_symbol(&err);
                                precounts.push(Err(err));
                                summaries.push(CountSummary {
                                    weight,
                                    n_words_allowed: 0,
                                    status: Status::Err,
                                    error_code: Some(code),
                                });
                            }
                        }
                    }
                    (Some(precounts), summaries)
                }
                Err(err) => {
                    let code = error_code_from_symbol(&err);
                    let mut summaries = Vec::with_capacity(range_len);
                    for weight in spec.weight_min..=spec.weight_max {
                        summaries.push(CountSummary {
                            weight,
                            n_words_allowed: 0,
                            status: Status::Err,
                            error_code: Some(code),
                        });
                    }
                    (None, summaries)
                }
            };
        let precounts = precounts.map(Arc::new);

        for weight in spec.weight_min..=spec.weight_max {
            let weight_idx = weight.saturating_sub(spec.weight_min);
            let count_summary = count_summaries
                .get(weight_idx)
                .ok_or_else(|| {
                    ExperimentError::InvalidConfig(
                        "count-only summary missing expected weight".to_string(),
                    )
                })?
                .clone();
            tasks.push(FiltrationTask {
                index: tasks.len(),
                layer_index,
                weight,
                weight_idx,
                layer_dir: layer_dir.clone(),
                count_summary,
                precounts: precounts.clone(),
                spec: spec.clone(),
                vars: vars.clone(),
            });
        }
    }

    let jobs = spec.jobs.unwrap_or(1);
    let mut results: Vec<(usize, Result<FiltrationSummaryRow, ExperimentError>)> = if jobs <= 1
        || tasks.len() <= 1
    {
        tasks
            .into_iter()
            .map(|task| (task.index, run_filtration_task(task)))
            .collect()
    } else {
        let pool = ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build()
            .map_err(|err| ExperimentError::InvalidConfig(format!("invalid engine.jobs: {err}")))?;
        pool.install(|| {
            tasks
                .par_iter()
                .cloned()
                .map(|task| (task.index, run_filtration_task(task)))
                .collect()
        })
    };

    results.sort_by_key(|(index, _)| *index);
    let mut rows = Vec::with_capacity(results.len());
    for (_index, result) in results {
        rows.push(result?);
    }

    Ok(FiltrationReport {
        name: spec.name.clone(),
        out_dir: spec.out_dir.clone(),
        weight_min: spec.weight_min,
        weight_max: spec.weight_max,
        layers,
        rows,
    })
}

fn run_filtration_task(task: FiltrationTask) -> Result<FiltrationSummaryRow, ExperimentError> {
    let layer = &task.spec.layers[task.layer_index];
    let weight_dir = task.layer_dir.join(format!("w{}", task.weight));
    fs::create_dir_all(&weight_dir)?;

    let cfg =
        build_experiment_config_for_layer(&task.spec, &task.vars, layer, task.weight, weight_dir);
    let count_summary = &task.count_summary;
    let count_report = CountReport {
        name: cfg.name.clone(),
        weight_min: task.weight,
        weight_max: task.weight,
        summaries: vec![count_summary.clone()],
    };

    let count_ok = count_summary.status == Status::Ok;
    let within_threshold = task
        .spec
        .full_run_max_words
        .is_none_or(|limit| (count_summary.n_words_allowed as u64) <= limit);
    let allow_full = match layer.mode {
        FiltrationMode::Full => count_ok,
        FiltrationMode::CountOnly => false,
        FiltrationMode::Auto => count_ok && within_threshold,
    };

    if !allow_full {
        write_count_only(&count_report, &cfg.out_dir)?;
    }

    let mut row = FiltrationSummaryRow {
        layer_index: task.layer_index,
        layer_name: layer.name.clone(),
        weight: task.weight,
        mode: layer.mode,
        status: count_summary.status,
        error_code: count_summary.error_code,
        n_words_allowed: count_summary.n_words_allowed,
        dim: None,
        rank: None,
        basis_ncols: None,
        rows_attempted: None,
        rows_inserted: None,
        samples_used: None,
        envs_total: None,
        constraints_insufficient_samples: None,
        sample_table: task.spec.sample_table,
    };

    let mut baseline_signature: Option<String> = None;
    let mut should_check_determinism = task.spec.repeats > 1 && row.status == Status::Ok;
    let precount_slice = task
        .precounts
        .as_ref()
        .map(|counts| &counts[task.weight_idx..task.weight_idx + 1]);

    if allow_full {
        let full_cfg = build_experiment_config_for_layer(
            &task.spec,
            &task.vars,
            layer,
            task.weight,
            cfg.out_dir.clone(),
        );
        let report = match precount_slice {
            Some(slice) => run_experiment_with_counts(&full_cfg, Some(slice))?,
            None => run_experiment(&full_cfg)?,
        };
        write_outputs(&report, &full_cfg.out_dir)?;
        let summary = report
            .summaries
            .iter()
            .find(|summary| summary.weight == task.weight)
            .ok_or_else(|| {
                ExperimentError::InvalidConfig(
                    "full run summary missing expected weight".to_string(),
                )
            })?;
        row.status = summary.status;
        row.error_code = summary.error_code;
        row.n_words_allowed = count_summary.n_words_allowed;
        if summary.status == Status::Ok {
            let stats = &summary.stats;
            row.dim = Some(stats.dim);
            row.rank = Some(stats.rank);
            row.basis_ncols = Some(stats.ncols);
            row.rows_attempted = Some(stats.rows_attempted as u64);
            row.rows_inserted = Some(stats.rows_inserted as u64);
            row.samples_used = Some(stats.samples_used as u64);
            row.envs_total = Some(stats.envs_total as u64);
            row.constraints_insufficient_samples =
                Some(stats.constraints_insufficient_samples as u64);
        }
        should_check_determinism = should_check_determinism && row.status == Status::Ok;
        if should_check_determinism {
            baseline_signature = Some(signature_from_full_report(&report));
        }
    } else if should_check_determinism {
        baseline_signature = Some(signature_from_count_report(&count_report));
    }

    if should_check_determinism {
        let base = baseline_signature.ok_or_else(|| {
            ExperimentError::InvalidConfig("repeat signature baseline missing".to_string())
        })?;
        let mut mismatch = false;
        for _repeat_idx in 1..task.spec.repeats {
            if allow_full {
                let full_cfg = build_experiment_config_for_layer(
                    &task.spec,
                    &task.vars,
                    layer,
                    task.weight,
                    cfg.out_dir.clone(),
                );
                let report = match precount_slice {
                    Some(slice) => run_experiment_with_counts(&full_cfg, Some(slice))?,
                    None => run_experiment(&full_cfg)?,
                };
                let signature = signature_from_full_report(&report);
                if signature != base {
                    mismatch = true;
                }
            } else {
                let repeat_count = run_count_only(&cfg)?;
                let signature = signature_from_count_report(&repeat_count);
                if signature != base {
                    mismatch = true;
                }
            }
        }
        if mismatch {
            row.status = Status::Err;
            row.error_code = Some(ErrorCode::NonDeterministicOutput);
        }
    }

    Ok(row)
}

fn build_experiment_config_for_layer(
    spec: &FiltrationSpec,
    vars: &[String],
    layer: &FiltrationLayer,
    weight: usize,
    out_dir: PathBuf,
) -> ExperimentConfig {
    ExperimentConfig {
        name: format!("{}::{}::w{}", spec.name, layer.name, weight),
        out_dir,
        alphabet: spec.alphabet.clone(),
        constraints: layer.constraints.clone(),
        genealogical_acceptors: layer.genealogical_acceptors.clone(),
        kgram_acceptors: layer.kgram_acceptors.clone(),
        channel_pairs_acceptors: layer.channel_pairs_acceptors.clone(),
        automaton_acceptors: layer.automaton_acceptors.clone(),
        constraint_budget: layer.constraint_budget,
        weight_min: weight,
        weight_max: weight,
        vars: vars.to_vec(),
        sample_table: spec.sample_table,
    }
}
