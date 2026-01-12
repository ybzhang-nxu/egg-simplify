use serde::Deserialize;

use mpl_symbol::space::SampleTable;

use crate::ExperimentError;

#[derive(Debug, Deserialize)]
pub(crate) struct SpecAlphabet {
    pub(crate) vars: Vec<String>,
    pub(crate) letters: Vec<SpecLetter>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SpecLetter {
    pub(crate) name: String,
    pub(crate) expr: String,
    pub(crate) channel: Option<SpecChannel>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum SpecChannel {
    Text(String),
    Int(u16),
}

#[derive(Debug, Deserialize)]
pub(crate) struct SpecConstraints {
    pub(crate) first_entry: Option<Vec<String>>,
    pub(crate) adjacency_mode: Option<String>,
    pub(crate) adjacency_pairs: Option<Vec<[String; 2]>>,
    pub(crate) budget: Option<SpecConstraintBudget>,
    pub(crate) automaton: Option<SpecAutomaton>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SpecConstraintBudget {
    pub(crate) max_states: Option<usize>,
    pub(crate) max_transitions: Option<usize>,
    pub(crate) max_words: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SpecAutomaton {
    pub(crate) acceptors: Option<Vec<SpecAutomatonAcceptor>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SpecGenealogicalRule {
    pub(crate) if_seen: String,
    pub(crate) forbid: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
pub(crate) enum SpecAutomatonAcceptor {
    #[serde(rename = "kgram")]
    KGram {
        k: usize,
        mode: String,
        triplets: Vec<[String; 3]>,
    },
    #[serde(rename = "channel_pairs")]
    ChannelPairs {
        mode: String,
        symmetric: Option<bool>,
        pairs: Vec<[SpecChannel; 2]>,
    },
    #[serde(rename = "genealogical")]
    Genealogical {
        seen: Option<String>,
        rules: Vec<SpecGenealogicalRule>,
    },
}

pub(crate) fn parse_sample_table(value: Option<&str>) -> Result<SampleTable, ExperimentError> {
    match value {
        None => Ok(SampleTable::default()),
        Some(name) => name
            .parse::<SampleTable>()
            .map_err(|_| ExperimentError::InvalidConfig(format!("unknown sample_table: {name}"))),
    }
}
