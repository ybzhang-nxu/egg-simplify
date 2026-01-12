use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::build::acceptors::{
    build_automaton_acceptors, validate_automaton_order, validate_channel_pairs_acceptors,
    validate_genealogical_acceptors, validate_kgram_acceptors,
};
use crate::build::alphabet::build_alphabet_from_spec;
use crate::build::constraints::{
    build_budget, build_constraints, build_engine_budget, merge_budget, validate_constraints,
};
use crate::run::filtration::{FiltrationLayer, FiltrationMode, FiltrationSpec};
use crate::spec::common::{
    parse_sample_table, SpecAlphabet, SpecConstraintBudget, SpecConstraints,
};
use crate::ExperimentError;

#[derive(Debug, Deserialize)]
struct FiltrationSpecFile {
    id: String,
    out_dir: String,
    alphabet: SpecAlphabet,
    weights: SpecWeights,
    engine: Option<SpecFiltrationEngine>,
    repeats: Option<usize>,
    layers: Vec<SpecFiltrationLayer>,
}

#[derive(Debug, Deserialize)]
struct SpecWeights {
    min: usize,
    max: usize,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SpecFiltrationEngine {
    pub(crate) budget: Option<SpecConstraintBudget>,
    pub(crate) full_run_max_words: Option<u64>,
    pub(crate) jobs: Option<usize>,
    pub(crate) sample_table: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SpecFiltrationLayer {
    name: String,
    mode: Option<String>,
    constraints: SpecConstraints,
}

pub fn load_filtration_spec(path: &Path) -> Result<FiltrationSpec, ExperimentError> {
    let content = fs::read_to_string(path)?;
    parse_filtration_spec_str(&content).map_err(|err| {
        ExperimentError::InvalidConfig(format!("filtration spec {}: {}", path.display(), err))
    })
}

pub fn parse_filtration_spec_str(input: &str) -> Result<FiltrationSpec, ExperimentError> {
    let spec: FiltrationSpecFile = toml::from_str(input)
        .map_err(|err| ExperimentError::InvalidConfig(format!("toml parse error: {err}")))?;

    if spec.weights.min > spec.weights.max {
        return Err(ExperimentError::InvalidConfig(
            "weights.min must be <= weights.max".to_string(),
        ));
    }

    let repeats = spec.repeats.unwrap_or(1);
    if repeats == 0 {
        return Err(ExperimentError::InvalidConfig(
            "repeats must be >= 1".to_string(),
        ));
    }

    if spec.layers.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "layers must be non-empty".to_string(),
        ));
    }

    let (alphabet, name_to_idx) = build_alphabet_from_spec(spec.id.clone(), &spec.alphabet)?;
    let engine_budget = build_engine_budget(spec.engine.as_ref());
    let sample_table = parse_sample_table(
        spec.engine
            .as_ref()
            .and_then(|engine| engine.sample_table.as_deref()),
    )?;
    let full_run_max_words = spec
        .engine
        .as_ref()
        .and_then(|engine| engine.full_run_max_words);
    let jobs = spec.engine.as_ref().and_then(|engine| engine.jobs);
    if let Some(value) = jobs {
        if value == 0 {
            return Err(ExperimentError::InvalidConfig(
                "engine.jobs must be >= 1".to_string(),
            ));
        }
    }

    let mut seen_layer_names = std::collections::BTreeSet::new();
    let mut layers = Vec::with_capacity(spec.layers.len());
    for layer in spec.layers {
        if !seen_layer_names.insert(layer.name.clone()) {
            return Err(ExperimentError::InvalidConfig(format!(
                "duplicate layer name: {}",
                layer.name
            )));
        }

        let mode = match layer.mode.as_deref() {
            Some("full") => FiltrationMode::Full,
            Some("count_only") => FiltrationMode::CountOnly,
            Some("auto") | None => FiltrationMode::Auto,
            Some(other) => {
                return Err(ExperimentError::InvalidConfig(format!(
                    "unknown layer mode: {other}"
                )))
            }
        };

        let constraints = build_constraints(&layer.constraints, &name_to_idx)?;
        let mut budget = build_budget(&layer.constraints);
        budget = merge_budget(&engine_budget, &budget);
        let (genealogical_acceptors, kgram_acceptors, channel_pairs_acceptors, automaton_acceptors) =
            build_automaton_acceptors(&layer.constraints, &name_to_idx, &alphabet.channels)?;

        let layer_config = FiltrationLayer {
            name: layer.name,
            mode,
            constraints,
            genealogical_acceptors,
            kgram_acceptors,
            channel_pairs_acceptors,
            automaton_acceptors,
            constraint_budget: budget,
        };

        validate_constraints(&alphabet, &layer_config.constraints)?;
        validate_genealogical_acceptors(&alphabet, &layer_config.genealogical_acceptors)?;
        validate_kgram_acceptors(&alphabet, &layer_config.kgram_acceptors)?;
        validate_channel_pairs_acceptors(&alphabet, &layer_config.channel_pairs_acceptors)?;
        validate_automaton_order(
            &layer_config.automaton_acceptors,
            &layer_config.kgram_acceptors,
            &layer_config.genealogical_acceptors,
            &layer_config.channel_pairs_acceptors,
        )?;

        layers.push(layer_config);
    }

    Ok(FiltrationSpec {
        name: spec.id,
        out_dir: spec.out_dir.into(),
        alphabet,
        weight_min: spec.weights.min,
        weight_max: spec.weights.max,
        vars: spec.alphabet.vars,
        repeats,
        full_run_max_words,
        jobs,
        sample_table,
        layers,
    })
}
