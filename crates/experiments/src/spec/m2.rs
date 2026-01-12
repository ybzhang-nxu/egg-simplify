use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::build::acceptors::build_automaton_acceptors;
use crate::build::alphabet::build_alphabet_from_spec;
use crate::build::constraints::{build_budget, build_constraints};
use crate::run::single::ExperimentConfig;
use crate::spec::common::{parse_sample_table, SpecAlphabet, SpecConstraints};
use crate::ExperimentError;

#[derive(Debug, Deserialize)]
struct SpecFile {
    experiment: SpecExperiment,
    alphabet: SpecAlphabet,
    constraints: SpecConstraints,
    engine: Option<SpecEngine>,
    pairs: Option<SpecPairs>,
}

#[derive(Debug, Deserialize)]
struct SpecExperiment {
    id: String,
    #[allow(dead_code)]
    title: Option<String>,
    out_dir: String,
    w_min: usize,
    w_max: usize,
}

#[derive(Debug, Deserialize)]
struct SpecPairs {
    count_mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SpecEngine {
    sample_table: Option<String>,
}

pub fn load_spec(path: &Path) -> Result<ExperimentConfig, ExperimentError> {
    let content = fs::read_to_string(path)?;
    parse_spec_str(&content)
        .map_err(|err| ExperimentError::InvalidConfig(format!("spec {}: {}", path.display(), err)))
}

pub fn parse_spec_str(input: &str) -> Result<ExperimentConfig, ExperimentError> {
    let spec: SpecFile = toml::from_str(input)
        .map_err(|err| ExperimentError::InvalidConfig(format!("toml parse error: {err}")))?;

    if spec.experiment.w_min > spec.experiment.w_max {
        return Err(ExperimentError::InvalidConfig(
            "w_min must be <= w_max".to_string(),
        ));
    }
    if let Some(pairs) = &spec.pairs {
        if let Some(mode) = &pairs.count_mode {
            if mode != "active_word_positions" {
                return Err(ExperimentError::InvalidConfig(format!(
                    "unsupported pairs.count_mode: {mode}"
                )));
            }
        }
    }

    let (alphabet, name_to_idx) =
        build_alphabet_from_spec(spec.experiment.id.clone(), &spec.alphabet)?;
    let sample_table = parse_sample_table(
        spec.engine
            .as_ref()
            .and_then(|engine| engine.sample_table.as_deref()),
    )?;

    let constraints = build_constraints(&spec.constraints, &name_to_idx)?;
    let constraint_budget = build_budget(&spec.constraints);
    let (genealogical_acceptors, kgram_acceptors, channel_pairs_acceptors, automaton_acceptors) =
        build_automaton_acceptors(&spec.constraints, &name_to_idx, &alphabet.channels)?;

    Ok(ExperimentConfig {
        name: spec.experiment.id,
        out_dir: spec.experiment.out_dir.into(),
        alphabet,
        constraints,
        genealogical_acceptors,
        kgram_acceptors,
        channel_pairs_acceptors,
        automaton_acceptors,
        constraint_budget,
        weight_min: spec.experiment.w_min,
        weight_max: spec.experiment.w_max,
        vars: spec.alphabet.vars,
        sample_table,
    })
}
