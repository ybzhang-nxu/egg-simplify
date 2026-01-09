use std::collections::{BTreeMap, BTreeSet};

use mpl_symbol::space::{
    ChannelPairsAcceptor, ChannelPairsMode, GenealogicalAcceptor, GenealogicalRule, KGramAcceptor,
    KGramMode, WordAcceptor, WordConstraints, WordConstraintsAcceptor,
};

use crate::spec::common::{SpecAutomatonAcceptor, SpecChannel, SpecConstraints};
use crate::ExperimentError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomatonAcceptorRef {
    KGram(usize),
    Genealogical(usize),
    ChannelPairs(usize),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AutomatonState {
    KGram(<KGramAcceptor as WordAcceptor>::State),
    Genealogical(<GenealogicalAcceptor as WordAcceptor>::State),
    ChannelPairs(<ChannelPairsAcceptor as WordAcceptor>::State),
}

pub(crate) struct CompositeAcceptor<'a> {
    base: WordConstraintsAcceptor<'a>,
    order: &'a [AutomatonAcceptorRef],
    kgrams: &'a [KGramAcceptor],
    genealogical: &'a [GenealogicalAcceptor],
    channel_pairs: &'a [ChannelPairsAcceptor],
}

impl<'a> CompositeAcceptor<'a> {
    pub(crate) fn new(
        constraints: &'a WordConstraints,
        order: &'a [AutomatonAcceptorRef],
        kgrams: &'a [KGramAcceptor],
        genealogical: &'a [GenealogicalAcceptor],
        channel_pairs: &'a [ChannelPairsAcceptor],
    ) -> Self {
        Self {
            base: WordConstraintsAcceptor::new(constraints),
            order,
            kgrams,
            genealogical,
            channel_pairs,
        }
    }
}

impl WordAcceptor for CompositeAcceptor<'_> {
    type State = (Option<usize>, Vec<Option<AutomatonState>>);

    fn start(&self) -> Self::State {
        let mut states = Vec::with_capacity(self.order.len());
        for entry in self.order {
            let state = match *entry {
                AutomatonAcceptorRef::KGram(idx) => self
                    .kgrams
                    .get(idx)
                    .map(|acceptor| AutomatonState::KGram(acceptor.start())),
                AutomatonAcceptorRef::Genealogical(idx) => self
                    .genealogical
                    .get(idx)
                    .map(|acceptor| AutomatonState::Genealogical(acceptor.start())),
                AutomatonAcceptorRef::ChannelPairs(idx) => self
                    .channel_pairs
                    .get(idx)
                    .map(|acceptor| AutomatonState::ChannelPairs(acceptor.start())),
            };
            states.push(state);
        }
        (self.base.start(), states)
    }

    fn step(&self, state: &Self::State, next: usize) -> Option<Self::State> {
        let base = self.base.step(&state.0, next)?;
        if state.1.len() != self.order.len() {
            return None;
        }
        let mut states = Vec::with_capacity(state.1.len());
        for (entry, sub_state) in self.order.iter().zip(state.1.iter()) {
            let sub_state = match sub_state {
                Some(inner) => inner,
                None => return None,
            };
            let next_state = match (entry, sub_state) {
                (AutomatonAcceptorRef::KGram(idx), AutomatonState::KGram(inner)) => {
                    let acceptor = self.kgrams.get(*idx)?;
                    Some(AutomatonState::KGram(acceptor.step(inner, next)?))
                }
                (AutomatonAcceptorRef::Genealogical(idx), AutomatonState::Genealogical(inner)) => {
                    let acceptor = self.genealogical.get(*idx)?;
                    Some(AutomatonState::Genealogical(acceptor.step(inner, next)?))
                }
                (AutomatonAcceptorRef::ChannelPairs(idx), AutomatonState::ChannelPairs(inner)) => {
                    let acceptor = self.channel_pairs.get(*idx)?;
                    Some(AutomatonState::ChannelPairs(acceptor.step(inner, next)?))
                }
                _ => return None,
            };
            states.push(next_state);
        }
        Some((base, states))
    }

    fn is_accepting(&self, state: &Self::State, depth: usize) -> bool {
        if !self.base.is_accepting(&state.0, depth) {
            return false;
        }
        if state.1.len() != self.order.len() {
            return false;
        }

        for (entry, sub_state) in self.order.iter().zip(state.1.iter()) {
            let sub_state = match sub_state {
                Some(inner) => inner,
                None => return false,
            };
            let ok = match (entry, sub_state) {
                (AutomatonAcceptorRef::KGram(idx), AutomatonState::KGram(inner)) => {
                    let acceptor = match self.kgrams.get(*idx) {
                        Some(acceptor) => acceptor,
                        None => return false,
                    };
                    acceptor.is_accepting(inner, depth)
                }
                (AutomatonAcceptorRef::Genealogical(idx), AutomatonState::Genealogical(inner)) => {
                    let acceptor = match self.genealogical.get(*idx) {
                        Some(acceptor) => acceptor,
                        None => return false,
                    };
                    acceptor.is_accepting(inner, depth)
                }
                (AutomatonAcceptorRef::ChannelPairs(idx), AutomatonState::ChannelPairs(inner)) => {
                    let acceptor = match self.channel_pairs.get(*idx) {
                        Some(acceptor) => acceptor,
                        None => return false,
                    };
                    acceptor.is_accepting(inner, depth)
                }
                _ => return false,
            };
            if !ok {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ChannelKey {
    Numeric(u16),
    Named(String),
}

impl ChannelKey {
    fn from_spec(spec: &SpecChannel) -> Self {
        match spec {
            SpecChannel::Int(value) => ChannelKey::Numeric(*value),
            SpecChannel::Text(value) => match value.parse::<u16>() {
                Ok(num) => ChannelKey::Numeric(num),
                Err(_) => ChannelKey::Named(value.clone()),
            },
        }
    }

    fn from_name(name: &str) -> Self {
        match name.parse::<u16>() {
            Ok(num) => ChannelKey::Numeric(num),
            Err(_) => ChannelKey::Named(name.to_string()),
        }
    }
}

type AutomatonBuild = (
    Vec<GenealogicalAcceptor>,
    Vec<KGramAcceptor>,
    Vec<ChannelPairsAcceptor>,
    Vec<AutomatonAcceptorRef>,
);

pub(crate) fn build_automaton_acceptors(
    spec: &SpecConstraints,
    name_to_idx: &BTreeMap<String, usize>,
    letter_channels: &[Option<SpecChannel>],
) -> Result<AutomatonBuild, ExperimentError> {
    const INVALID_SPEC_MISSING_CHANNEL: &str = "InvalidSpecMissingChannel";
    const INVALID_SPEC_UNKNOWN_MODE: &str = "InvalidSpecUnknownGenealogicalMode";
    const INVALID_SPEC_UNKNOWN_CHANNEL: &str = "InvalidSpecUnknownChannel";
    const INVALID_SPEC_UNKNOWN_LETTER: &str = "InvalidSpecUnknownLetter";
    const INVALID_SPEC_DUPLICATE_RULE: &str = "InvalidSpecDuplicateRule";
    const INVALID_SPEC_DUPLICATE_FORBID: &str = "InvalidSpecDuplicateForbid";
    const INVALID_SPEC_EMPTY_ALLOW_LIST: &str = "InvalidSpecEmptyAllowList";
    const INVALID_SPEC_NON_NUMERIC_CHANNEL: &str = "InvalidSpecNonNumericChannel";

    let Some(automaton) = &spec.automaton else {
        return Ok((Vec::new(), Vec::new(), Vec::new(), Vec::new()));
    };

    let mut genealogical = Vec::new();
    let mut kgrams = Vec::new();
    let mut channel_pairs = Vec::new();
    let mut order = Vec::new();
    let mut channel_cache: Option<(BTreeMap<ChannelKey, usize>, Vec<usize>)> = None;

    let acceptors = automaton.acceptors.as_deref().unwrap_or(&[]);
    for acceptor in acceptors {
        match acceptor {
            SpecAutomatonAcceptor::Genealogical { seen, rules } => {
                let mode = seen.as_deref().unwrap_or("channel");
                let mut channel_map: Option<&BTreeMap<ChannelKey, usize>> = None;
                let (letter_to_key, key_count) = match mode {
                    "channel" => {
                        if channel_cache.is_none() {
                            let (key_map, letter_to_channel) =
                                build_channel_key_map(letter_channels)?;
                            channel_cache = Some((key_map, letter_to_channel));
                        }
                        let (key_map, letter_to_channel) = match channel_cache.as_ref() {
                            Some(value) => value,
                            None => {
                                return Err(ExperimentError::InvalidConfig(format!(
                                    "{INVALID_SPEC_MISSING_CHANNEL}: channel map missing"
                                )))
                            }
                        };
                        channel_map = Some(key_map);
                        (letter_to_channel.clone(), key_map.len())
                    }
                    "letter" => {
                        let mut letter_to_key = Vec::with_capacity(name_to_idx.len());
                        for idx in 0..name_to_idx.len() {
                            letter_to_key.push(idx);
                        }
                        (letter_to_key, name_to_idx.len())
                    }
                    other => {
                        return Err(ExperimentError::InvalidConfig(format!(
                            "{INVALID_SPEC_UNKNOWN_MODE}: {other}"
                        )))
                    }
                };

                if rules.is_empty() {
                    continue;
                }

                let mut mapped_rules = Vec::with_capacity(rules.len());
                let mut seen_rules = BTreeSet::new();
                for rule in rules {
                    let if_seen = match mode {
                        "channel" => resolve_channel_name(
                            &rule.if_seen,
                            channel_map,
                            INVALID_SPEC_UNKNOWN_CHANNEL,
                        )?,
                        "letter" => resolve_letter_name(
                            &rule.if_seen,
                            name_to_idx,
                            INVALID_SPEC_UNKNOWN_LETTER,
                        )?,
                        _ => {
                            return Err(ExperimentError::InvalidConfig(format!(
                                "{INVALID_SPEC_UNKNOWN_MODE}: {mode}"
                            )))
                        }
                    };
                    let mut forbid = Vec::with_capacity(rule.forbid.len());
                    for name in &rule.forbid {
                        let idx = match mode {
                            "channel" => resolve_channel_name(
                                name,
                                channel_map,
                                INVALID_SPEC_UNKNOWN_CHANNEL,
                            )?,
                            "letter" => {
                                resolve_letter_name(name, name_to_idx, INVALID_SPEC_UNKNOWN_LETTER)?
                            }
                            _ => {
                                return Err(ExperimentError::InvalidConfig(format!(
                                    "{INVALID_SPEC_UNKNOWN_MODE}: {mode}"
                                )))
                            }
                        };
                        forbid.push(idx);
                    }
                    forbid.sort_unstable();
                    for window in forbid.windows(2) {
                        if window[0] == window[1] {
                            return Err(ExperimentError::InvalidConfig(format!(
                                "{INVALID_SPEC_DUPLICATE_FORBID}: {}",
                                rule.if_seen
                            )));
                        }
                    }
                    if !seen_rules.insert((if_seen, forbid.clone())) {
                        return Err(ExperimentError::InvalidConfig(format!(
                            "{INVALID_SPEC_DUPLICATE_RULE}: {}",
                            rule.if_seen
                        )));
                    }
                    mapped_rules.push(GenealogicalRule { if_seen, forbid });
                }

                let acceptor = GenealogicalAcceptor::new(letter_to_key, key_count, mapped_rules)
                    .map_err(|err| {
                        ExperimentError::InvalidConfig(format!("genealogical error: {err}"))
                    })?;
                genealogical.push(acceptor);
                let idx = genealogical.len() - 1;
                order.push(AutomatonAcceptorRef::Genealogical(idx));
            }
            SpecAutomatonAcceptor::KGram { k, mode, triplets } => {
                if *k != 3 {
                    return Err(ExperimentError::InvalidConfig(format!(
                        "kgram acceptor requires k=3 (got {k})"
                    )));
                }
                let mode = match mode.as_str() {
                    "allowed" => KGramMode::Allowed,
                    "forbidden" => KGramMode::Forbidden,
                    other => {
                        return Err(ExperimentError::InvalidConfig(format!(
                            "unknown kgram mode: {other}"
                        )))
                    }
                };
                if mode == KGramMode::Allowed && triplets.is_empty() {
                    return Err(ExperimentError::InvalidConfig(format!(
                        "{INVALID_SPEC_EMPTY_ALLOW_LIST}: kgram mode=allowed requires non-empty triplets"
                    )));
                }
                let mut ids = Vec::with_capacity(triplets.len());
                for triplet in triplets {
                    let a_name = &triplet[0];
                    let b_name = &triplet[1];
                    let c_name = &triplet[2];
                    let a = name_to_idx.get(a_name).ok_or_else(|| {
                        ExperimentError::InvalidConfig(format!(
                            "kgram triplet references unknown letter: {a_name}"
                        ))
                    })?;
                    let b = name_to_idx.get(b_name).ok_or_else(|| {
                        ExperimentError::InvalidConfig(format!(
                            "kgram triplet references unknown letter: {b_name}"
                        ))
                    })?;
                    let c = name_to_idx.get(c_name).ok_or_else(|| {
                        ExperimentError::InvalidConfig(format!(
                            "kgram triplet references unknown letter: {c_name}"
                        ))
                    })?;
                    ids.push([*a, *b, *c]);
                }
                let acceptor = KGramAcceptor::new(mode, ids).map_err(|err| {
                    ExperimentError::InvalidConfig(format!("kgram acceptor error: {err}"))
                })?;
                kgrams.push(acceptor);
                let idx = kgrams.len() - 1;
                order.push(AutomatonAcceptorRef::KGram(idx));
            }
            SpecAutomatonAcceptor::ChannelPairs {
                mode,
                symmetric,
                pairs,
            } => {
                let mode = match mode.as_str() {
                    "allowed" => ChannelPairsMode::Allowed,
                    "forbidden" => ChannelPairsMode::Forbidden,
                    other => {
                        return Err(ExperimentError::InvalidConfig(format!(
                            "unknown channel_pairs mode: {other}"
                        )))
                    }
                };
                if mode == ChannelPairsMode::Allowed && pairs.is_empty() {
                    return Err(ExperimentError::InvalidConfig(format!(
                        "{INVALID_SPEC_EMPTY_ALLOW_LIST}: channel_pairs mode=allowed requires non-empty pairs"
                    )));
                }

                let (letter_to_channel, channel_ids) = build_channel_u16_map(
                    letter_channels,
                    INVALID_SPEC_MISSING_CHANNEL,
                    INVALID_SPEC_NON_NUMERIC_CHANNEL,
                )?;

                for pair in pairs {
                    for value in pair {
                        if !channel_ids.contains(value) {
                            return Err(ExperimentError::InvalidConfig(format!(
                                "{INVALID_SPEC_UNKNOWN_CHANNEL}: {value}"
                            )));
                        }
                    }
                }

                let acceptor = ChannelPairsAcceptor::new(
                    letter_to_channel,
                    mode,
                    symmetric.unwrap_or(false),
                    pairs.clone(),
                )
                .map_err(|err| {
                    ExperimentError::InvalidConfig(format!("channel_pairs acceptor error: {err}"))
                })?;
                channel_pairs.push(acceptor);
                let idx = channel_pairs.len() - 1;
                order.push(AutomatonAcceptorRef::ChannelPairs(idx));
            }
        }
    }
    Ok((genealogical, kgrams, channel_pairs, order))
}

fn build_channel_key_map(
    letter_channels: &[Option<SpecChannel>],
) -> Result<(BTreeMap<ChannelKey, usize>, Vec<usize>), ExperimentError> {
    const INVALID_SPEC_MISSING_CHANNEL: &str = "InvalidSpecMissingChannel";

    let mut channel_keys = BTreeSet::new();
    let mut per_letter = Vec::with_capacity(letter_channels.len());
    for channel in letter_channels {
        let Some(value) = channel else {
            return Err(ExperimentError::InvalidConfig(format!(
                "{INVALID_SPEC_MISSING_CHANNEL}: missing channel on letter"
            )));
        };
        match value {
            SpecChannel::Text(text) if text.is_empty() => {
                return Err(ExperimentError::InvalidConfig(format!(
                    "{INVALID_SPEC_MISSING_CHANNEL}: empty channel on letter"
                )));
            }
            _ => {}
        }
        let key = ChannelKey::from_spec(value);
        channel_keys.insert(key.clone());
        per_letter.push(key);
    }

    let mut key_map = BTreeMap::new();
    for (idx, key) in channel_keys.into_iter().enumerate() {
        key_map.insert(key, idx);
    }

    let mut letter_to_key = Vec::with_capacity(per_letter.len());
    for key in per_letter {
        let idx = key_map.get(&key).copied().ok_or_else(|| {
            ExperimentError::InvalidConfig(format!(
                "{INVALID_SPEC_MISSING_CHANNEL}: channel map missing"
            ))
        })?;
        letter_to_key.push(idx);
    }

    Ok((key_map, letter_to_key))
}

fn build_channel_u16_map(
    letter_channels: &[Option<SpecChannel>],
    missing_code: &str,
    non_numeric_code: &str,
) -> Result<(Vec<u16>, BTreeSet<u16>), ExperimentError> {
    let mut channel_ids = BTreeSet::new();
    let mut letter_to_channel = Vec::with_capacity(letter_channels.len());
    for (idx, channel) in letter_channels.iter().enumerate() {
        let Some(value) = channel else {
            return Err(ExperimentError::InvalidConfig(format!(
                "{missing_code}: missing channel on letter {idx}"
            )));
        };
        let id = match value {
            SpecChannel::Int(value) => *value,
            SpecChannel::Text(text) => text.parse::<u16>().map_err(|_| {
                ExperimentError::InvalidConfig(format!(
                    "{non_numeric_code}: non-numeric channel on letter {idx}"
                ))
            })?,
        };
        channel_ids.insert(id);
        letter_to_channel.push(id);
    }
    Ok((letter_to_channel, channel_ids))
}

fn resolve_channel_name(
    name: &str,
    channel_map: Option<&BTreeMap<ChannelKey, usize>>,
    unknown_code: &str,
) -> Result<usize, ExperimentError> {
    let map = channel_map.ok_or_else(|| {
        ExperimentError::InvalidConfig("InvalidSpecMissingChannel: channel map missing".to_string())
    })?;
    let key = ChannelKey::from_name(name);
    map.get(&key)
        .copied()
        .ok_or_else(|| ExperimentError::InvalidConfig(format!("{unknown_code}: {name}")))
}

fn resolve_letter_name(
    name: &str,
    name_to_idx: &BTreeMap<String, usize>,
    unknown_code: &str,
) -> Result<usize, ExperimentError> {
    name_to_idx
        .get(name)
        .copied()
        .ok_or_else(|| ExperimentError::InvalidConfig(format!("{unknown_code}: {name}")))
}

pub(crate) fn validate_genealogical_acceptors(
    alphabet: &mpl_symbol::space::Alphabet,
    acceptors: &[GenealogicalAcceptor],
) -> Result<(), ExperimentError> {
    let size = alphabet.letters.len();
    for acceptor in acceptors {
        if acceptor.letter_count() != size {
            return Err(ExperimentError::InvalidConfig(
                "genealogical acceptor letter mapping mismatch".to_string(),
            ));
        }
        if acceptor.key_count() == 0 && size > 0 {
            return Err(ExperimentError::InvalidConfig(
                "genealogical acceptor has zero keys".to_string(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_automaton_order(
    order: &[AutomatonAcceptorRef],
    kgrams: &[KGramAcceptor],
    genealogical: &[GenealogicalAcceptor],
    channel_pairs: &[ChannelPairsAcceptor],
) -> Result<(), ExperimentError> {
    for entry in order {
        match *entry {
            AutomatonAcceptorRef::KGram(idx) => {
                if idx >= kgrams.len() {
                    return Err(ExperimentError::InvalidConfig(
                        "automaton order references missing kgram acceptor".to_string(),
                    ));
                }
            }
            AutomatonAcceptorRef::Genealogical(idx) => {
                if idx >= genealogical.len() {
                    return Err(ExperimentError::InvalidConfig(
                        "automaton order references missing genealogical acceptor".to_string(),
                    ));
                }
            }
            AutomatonAcceptorRef::ChannelPairs(idx) => {
                if idx >= channel_pairs.len() {
                    return Err(ExperimentError::InvalidConfig(
                        "automaton order references missing channel_pairs acceptor".to_string(),
                    ));
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_kgram_acceptors(
    alphabet: &mpl_symbol::space::Alphabet,
    acceptors: &[KGramAcceptor],
) -> Result<(), ExperimentError> {
    let size = alphabet.letters.len();
    for acceptor in acceptors {
        for triplet in acceptor.triplets() {
            if triplet.iter().any(|&idx| idx >= size) {
                return Err(ExperimentError::InvalidConfig(
                    "kgram triplet index out of range".to_string(),
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_channel_pairs_acceptors(
    alphabet: &mpl_symbol::space::Alphabet,
    acceptors: &[ChannelPairsAcceptor],
) -> Result<(), ExperimentError> {
    let size = alphabet.letters.len();
    for acceptor in acceptors {
        if acceptor.letter_count() != size {
            return Err(ExperimentError::InvalidConfig(
                "channel_pairs acceptor letter mapping mismatch".to_string(),
            ));
        }
    }
    Ok(())
}
