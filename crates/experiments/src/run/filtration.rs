use std::fs;
use std::path::PathBuf;

use mpl_symbol::space::{
    Alphabet, ChannelPairsAcceptor, ConstraintBudget, GenealogicalAcceptor, KGramAcceptor,
    WordConstraints,
};

use crate::build::acceptors::AutomatonAcceptorRef;
use crate::build::alphabet::collect_vars_from_letters;
use crate::output::single::{write_count_only, write_outputs};
use crate::run::count::run_count_only;
use crate::run::single::{run_experiment, ExperimentConfig};
use crate::util::sanitize::sanitize_layer_name;
use crate::util::signature::{filtration_temp_dir, read_signature};
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

    let vars = if spec.vars.is_empty() {
        collect_vars_from_letters(&spec.alphabet.letters)
    } else {
        spec.vars.clone()
    };

    fs::create_dir_all(&spec.out_dir)?;
    let layers_dir = spec.out_dir.join("layers");
    fs::create_dir_all(&layers_dir)?;

    let mut layers = Vec::with_capacity(spec.layers.len());
    let mut rows = Vec::new();

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

        for weight in spec.weight_min..=spec.weight_max {
            let weight_dir = layer_dir.join(format!("w{weight}"));
            fs::create_dir_all(&weight_dir)?;

            let cfg = build_experiment_config_for_layer(spec, &vars, layer, weight, weight_dir);
            let count_report = run_count_only(&cfg)?;
            let count_summary = count_report
                .summaries
                .iter()
                .find(|summary| summary.weight == weight)
                .ok_or_else(|| {
                    ExperimentError::InvalidConfig(
                        "count-only summary missing expected weight".to_string(),
                    )
                })?;

            let count_ok = count_summary.status == Status::Ok;
            let within_threshold = spec
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
                layer_index,
                layer_name: layer.name.clone(),
                weight,
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
            };

            let mut baseline_signature: Option<String> = None;
            let mut should_check_determinism = spec.repeats > 1 && row.status == Status::Ok;

            if allow_full {
                let full_cfg = build_experiment_config_for_layer(
                    spec,
                    &vars,
                    layer,
                    weight,
                    cfg.out_dir.clone(),
                );
                let report = run_experiment(&full_cfg)?;
                write_outputs(&report, &full_cfg.out_dir)?;
                let summary = report
                    .summaries
                    .iter()
                    .find(|summary| summary.weight == weight)
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
                    baseline_signature = Some(read_signature(&full_cfg.out_dir, true)?);
                }
            } else if should_check_determinism {
                baseline_signature = Some(read_signature(&cfg.out_dir, false)?);
            }

            if should_check_determinism {
                let base = baseline_signature.ok_or_else(|| {
                    ExperimentError::InvalidConfig("repeat signature baseline missing".to_string())
                })?;
                let mut mismatch = false;
                for repeat_idx in 1..spec.repeats {
                    let temp_dir = filtration_temp_dir(&spec.name, layer_index, weight, repeat_idx);
                    fs::create_dir_all(&temp_dir)?;

                    let repeat_cfg = build_experiment_config_for_layer(
                        spec,
                        &vars,
                        layer,
                        weight,
                        temp_dir.clone(),
                    );

                    if allow_full {
                        let report = run_experiment(&repeat_cfg)?;
                        write_outputs(&report, &temp_dir)?;
                        let signature = read_signature(&temp_dir, true)?;
                        if signature != base {
                            mismatch = true;
                        }
                    } else {
                        let repeat_count = run_count_only(&repeat_cfg)?;
                        write_count_only(&repeat_count, &temp_dir)?;
                        let signature = read_signature(&temp_dir, false)?;
                        if signature != base {
                            mismatch = true;
                        }
                    }

                    let _ = fs::remove_dir_all(&temp_dir);
                }
                if mismatch {
                    row.status = Status::Err;
                    row.error_code = Some(ErrorCode::NonDeterministicOutput);
                }
            }

            rows.push(row);
        }
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
    }
}
