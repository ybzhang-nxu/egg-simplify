use std::collections::{BTreeMap, BTreeSet};

use mpl_symbol::space::{Alphabet, ConstraintBudget, WordConstraints};

use crate::spec::common::SpecConstraints;
use crate::spec::m6::SpecFiltrationEngine;
use crate::ExperimentError;

pub(crate) fn build_constraints(
    spec: &SpecConstraints,
    name_to_idx: &BTreeMap<String, usize>,
) -> Result<WordConstraints, ExperimentError> {
    let size = name_to_idx.len();
    let first_allowed = match &spec.first_entry {
        Some(entries) => {
            let mut set = BTreeSet::new();
            for name in entries {
                let idx = name_to_idx.get(name).ok_or_else(|| {
                    ExperimentError::InvalidConfig(format!(
                        "first_entry references unknown letter: {name}"
                    ))
                })?;
                set.insert(*idx);
            }
            Some(set)
        }
        None => None,
    };

    let mode = spec.adjacency_mode.as_deref().ok_or_else(|| {
        ExperimentError::InvalidConfig("constraints.adjacency_mode is required".to_string())
    })?;
    let pairs = spec.adjacency_pairs.clone().unwrap_or_default();
    if mode == "allow" && pairs.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "adjacency_mode=allow requires non-empty adjacency_pairs".to_string(),
        ));
    }
    let mut allowed_pairs = match mode {
        "allow" => vec![vec![false; size]; size],
        "forbid" => vec![vec![true; size]; size],
        other => {
            return Err(ExperimentError::InvalidConfig(format!(
                "unknown adjacency_mode: {other}"
            )))
        }
    };

    for pair in pairs {
        let a_name = &pair[0];
        let b_name = &pair[1];
        let a = name_to_idx.get(a_name).ok_or_else(|| {
            ExperimentError::InvalidConfig(format!(
                "adjacency_pairs references unknown letter: {a_name}"
            ))
        })?;
        let b = name_to_idx.get(b_name).ok_or_else(|| {
            ExperimentError::InvalidConfig(format!(
                "adjacency_pairs references unknown letter: {b_name}"
            ))
        })?;
        allowed_pairs[*a][*b] = mode == "allow";
    }

    Ok(WordConstraints {
        first_allowed,
        allowed_pairs: Some(allowed_pairs),
    })
}

pub(crate) fn build_budget(spec: &SpecConstraints) -> ConstraintBudget {
    let mut budget = ConstraintBudget::default();
    if let Some(spec_budget) = &spec.budget {
        budget.max_states = spec_budget.max_states;
        budget.max_transitions = spec_budget.max_transitions;
        budget.max_words = spec_budget.max_words;
    }
    budget
}

pub(crate) fn build_engine_budget(spec: Option<&SpecFiltrationEngine>) -> ConstraintBudget {
    let mut budget = ConstraintBudget::default();
    if let Some(engine) = spec {
        if let Some(engine_budget) = &engine.budget {
            budget.max_states = engine_budget.max_states;
            budget.max_transitions = engine_budget.max_transitions;
            budget.max_words = engine_budget.max_words;
        }
    }
    budget
}

pub(crate) fn merge_budget(
    base: &ConstraintBudget,
    overrides: &ConstraintBudget,
) -> ConstraintBudget {
    ConstraintBudget {
        max_states: overrides.max_states.or(base.max_states),
        max_transitions: overrides.max_transitions.or(base.max_transitions),
        max_words: overrides.max_words.or(base.max_words),
    }
}

pub(crate) fn validate_constraints(
    alphabet: &Alphabet,
    constraints: &WordConstraints,
) -> Result<(), ExperimentError> {
    let size = alphabet.letters.len();
    if let Some(first) = &constraints.first_allowed {
        if first.iter().any(|&idx| idx >= size) {
            return Err(ExperimentError::InvalidConfig(
                "first_allowed constraint out of range".to_string(),
            ));
        }
    }
    if let Some(pairs) = &constraints.allowed_pairs {
        if pairs.len() != size || pairs.iter().any(|row| row.len() != size) {
            return Err(ExperimentError::InvalidConfig(
                "allowed_pairs adjacency matrix mismatch".to_string(),
            ));
        }
    }
    Ok(())
}
