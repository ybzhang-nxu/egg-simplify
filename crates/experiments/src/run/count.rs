use crate::build::acceptors::{
    validate_automaton_order, validate_channel_pairs_acceptors, validate_genealogical_acceptors,
    validate_kgram_acceptors, CompositeAcceptor,
};
use crate::build::alphabet::normalize_inputs;
use crate::build::constraints::validate_constraints;
use crate::run::single::{convert_word_count, error_code_from_symbol, ExperimentConfig};
use crate::{ErrorCode, ExperimentError, Status};
use mpl_symbol::space::build_word_count_cache;

#[derive(Clone, Debug)]
pub struct CountReport {
    pub name: String,
    pub weight_min: usize,
    pub weight_max: usize,
    pub summaries: Vec<CountSummary>,
}

#[derive(Clone, Debug)]
pub struct CountSummary {
    pub weight: usize,
    pub n_words_allowed: usize,
    pub status: Status,
    pub error_code: Option<ErrorCode>,
}

pub fn run_count_only(cfg: &ExperimentConfig) -> Result<CountReport, ExperimentError> {
    if cfg.weight_min > cfg.weight_max {
        return Err(ExperimentError::InvalidConfig(
            "weight_min must be <= weight_max".to_string(),
        ));
    }

    let (alphabet, constraints) = normalize_inputs(&cfg.alphabet, &cfg.constraints);
    validate_constraints(&alphabet, &constraints)?;
    validate_genealogical_acceptors(&alphabet, &cfg.genealogical_acceptors)?;
    validate_kgram_acceptors(&alphabet, &cfg.kgram_acceptors)?;
    validate_channel_pairs_acceptors(&alphabet, &cfg.channel_pairs_acceptors)?;
    validate_automaton_order(
        &cfg.automaton_acceptors,
        &cfg.kgram_acceptors,
        &cfg.genealogical_acceptors,
        &cfg.channel_pairs_acceptors,
    )?;

    let alpha_len = alphabet.letters.len();
    let acceptor = CompositeAcceptor::new(
        &constraints,
        &cfg.automaton_acceptors,
        &cfg.kgram_acceptors,
        &cfg.genealogical_acceptors,
        &cfg.channel_pairs_acceptors,
    );
    let budget = cfg.constraint_budget;

    let mut summaries = Vec::new();
    let mut cache = match build_word_count_cache(alpha_len, &acceptor, cfg.weight_max, budget) {
        Ok(cache) => cache,
        Err(err) => {
            let code = error_code_from_symbol(&err);
            for weight in cfg.weight_min..=cfg.weight_max {
                summaries.push(CountSummary {
                    weight,
                    n_words_allowed: 0,
                    status: Status::Err,
                    error_code: Some(code),
                });
            }
            return Ok(CountReport {
                name: cfg.name.clone(),
                weight_min: cfg.weight_min,
                weight_max: cfg.weight_max,
                summaries,
            });
        }
    };

    let results = cache.count_range(cfg.weight_min, cfg.weight_max);
    for (weight, result) in (cfg.weight_min..=cfg.weight_max).zip(results) {
        match result.and_then(convert_word_count) {
            Ok(count) => summaries.push(CountSummary {
                weight,
                n_words_allowed: count,
                status: Status::Ok,
                error_code: None,
            }),
            Err(err) => summaries.push(CountSummary {
                weight,
                n_words_allowed: 0,
                status: Status::Err,
                error_code: Some(error_code_from_symbol(&err)),
            }),
        }
    }

    Ok(CountReport {
        name: cfg.name.clone(),
        weight_min: cfg.weight_min,
        weight_max: cfg.weight_max,
        summaries,
    })
}
