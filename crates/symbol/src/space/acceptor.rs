use super::WordConstraints;
use crate::error::SymbolError;

#[derive(Clone, Copy, Debug, Default)]
pub struct ConstraintBudget {
    pub max_states: Option<usize>,
    pub max_transitions: Option<usize>,
    pub max_words: Option<u64>,
}

pub trait WordAcceptor {
    type State: Clone + Ord;

    fn start(&self) -> Self::State;
    fn step(&self, state: &Self::State, next: usize) -> Option<Self::State>;
    fn is_accepting(&self, _state: &Self::State, _depth: usize) -> bool {
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KGramMode {
    Allowed,
    Forbidden,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Bitset {
    words: Vec<u64>,
}

impl Bitset {
    fn new(bits: usize) -> Self {
        let words = bits.saturating_add(63) / 64;
        Self {
            words: vec![0u64; words],
        }
    }

    fn contains(&self, idx: usize) -> bool {
        let word = idx / 64;
        if word >= self.words.len() {
            return false;
        }
        let bit = idx % 64;
        (self.words[word] & (1u64 << bit)) != 0
    }

    fn set(&mut self, idx: usize) {
        let word = idx / 64;
        if word >= self.words.len() {
            return;
        }
        let bit = idx % 64;
        self.words[word] |= 1u64 << bit;
    }

    fn or_assign(&mut self, other: &Bitset) {
        if self.words.len() != other.words.len() {
            return;
        }
        for (left, right) in self.words.iter_mut().zip(other.words.iter()) {
            *left |= *right;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenealogicalRule {
    pub if_seen: usize,
    pub forbid: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GenealogicalState {
    seen: Bitset,
    forbidden: Bitset,
}

#[derive(Clone, Debug)]
pub struct GenealogicalAcceptor {
    letter_to_key: Vec<usize>,
    forbid_masks: Vec<Bitset>,
    key_count: usize,
}

impl GenealogicalAcceptor {
    pub fn new(
        letter_to_key: Vec<usize>,
        key_count: usize,
        rules: Vec<GenealogicalRule>,
    ) -> Result<Self, SymbolError> {
        if key_count == 0 && !letter_to_key.is_empty() {
            return Err(SymbolError::NotImplemented(
                "genealogical acceptor has zero keys".to_string(),
            ));
        }
        if letter_to_key.iter().any(|&key| key >= key_count) {
            return Err(SymbolError::NotImplemented(
                "genealogical acceptor letter mapping out of range".to_string(),
            ));
        }

        let mut forbid_masks = Vec::with_capacity(key_count);
        for _ in 0..key_count {
            forbid_masks.push(Bitset::new(key_count));
        }

        let mut seen_rules = std::collections::BTreeSet::new();
        for rule in rules {
            if rule.if_seen >= key_count {
                return Err(SymbolError::NotImplemented(
                    "genealogical rule if_seen out of range".to_string(),
                ));
            }
            let mut forbid = rule.forbid;
            forbid.sort_unstable();
            if forbid.iter().any(|&idx| idx >= key_count) {
                return Err(SymbolError::NotImplemented(
                    "genealogical rule forbid out of range".to_string(),
                ));
            }
            for window in forbid.windows(2) {
                if window[0] == window[1] {
                    return Err(SymbolError::NotImplemented(
                        "duplicate genealogical rule forbid entry".to_string(),
                    ));
                }
            }
            if !seen_rules.insert((rule.if_seen, forbid.clone())) {
                return Err(SymbolError::NotImplemented(
                    "duplicate genealogical rule".to_string(),
                ));
            }
            let mask = &mut forbid_masks[rule.if_seen];
            for idx in forbid {
                mask.set(idx);
            }
        }

        Ok(Self {
            letter_to_key,
            forbid_masks,
            key_count,
        })
    }

    pub fn letter_count(&self) -> usize {
        self.letter_to_key.len()
    }

    pub fn key_count(&self) -> usize {
        self.key_count
    }
}

impl WordAcceptor for GenealogicalAcceptor {
    type State = GenealogicalState;

    fn start(&self) -> Self::State {
        GenealogicalState {
            seen: Bitset::new(self.key_count),
            forbidden: Bitset::new(self.key_count),
        }
    }

    fn step(&self, state: &Self::State, next: usize) -> Option<Self::State> {
        let key = *self.letter_to_key.get(next)?;
        if key >= self.key_count {
            return None;
        }
        if state.forbidden.contains(key) {
            return None;
        }

        let mut seen = state.seen.clone();
        seen.set(key);

        let mut forbidden = state.forbidden.clone();
        if let Some(mask) = self.forbid_masks.get(key) {
            forbidden.or_assign(mask);
        }

        Some(GenealogicalState { seen, forbidden })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct KGramState {
    prev2: Option<usize>,
    prev1: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct KGramAcceptor {
    mode: KGramMode,
    triplets: Vec<[usize; 3]>,
}

impl KGramAcceptor {
    pub fn new(mode: KGramMode, mut triplets: Vec<[usize; 3]>) -> Result<Self, SymbolError> {
        triplets.sort_unstable();
        for window in triplets.windows(2) {
            if window[0] == window[1] {
                return Err(SymbolError::NotImplemented(
                    "duplicate k-gram triplet".to_string(),
                ));
            }
        }
        Ok(Self { mode, triplets })
    }

    pub fn mode(&self) -> KGramMode {
        self.mode
    }

    pub fn triplets(&self) -> &[[usize; 3]] {
        &self.triplets
    }
}

impl WordAcceptor for KGramAcceptor {
    type State = KGramState;

    fn start(&self) -> Self::State {
        KGramState::default()
    }

    fn step(&self, state: &Self::State, next: usize) -> Option<Self::State> {
        if let (Some(a), Some(b)) = (state.prev2, state.prev1) {
            let key = [a, b, next];
            let contains = self.triplets.binary_search(&key).is_ok();
            match self.mode {
                KGramMode::Allowed if !contains => return None,
                KGramMode::Forbidden if contains => return None,
                _ => {}
            }
        }

        Some(KGramState {
            prev2: state.prev1,
            prev1: Some(next),
        })
    }
}

impl<T: WordAcceptor + ?Sized> WordAcceptor for &T {
    type State = T::State;

    fn start(&self) -> Self::State {
        (*self).start()
    }

    fn step(&self, state: &Self::State, next: usize) -> Option<Self::State> {
        (*self).step(state, next)
    }

    fn is_accepting(&self, state: &Self::State, depth: usize) -> bool {
        (*self).is_accepting(state, depth)
    }
}

pub struct WordConstraintsAcceptor<'a> {
    constraints: &'a WordConstraints,
}

impl<'a> WordConstraintsAcceptor<'a> {
    pub fn new(constraints: &'a WordConstraints) -> Self {
        Self { constraints }
    }
}

impl WordAcceptor for WordConstraintsAcceptor<'_> {
    type State = Option<usize>;

    fn start(&self) -> Self::State {
        None
    }

    fn step(&self, state: &Self::State, next: usize) -> Option<Self::State> {
        if state.is_none() {
            if let Some(first) = &self.constraints.first_allowed {
                if !first.contains(&next) {
                    return None;
                }
            }
        }

        if let Some(prev_idx) = *state {
            if let Some(pairs) = &self.constraints.allowed_pairs {
                if prev_idx >= pairs.len() {
                    return None;
                }
                let row = &pairs[prev_idx];
                if next >= row.len() {
                    return None;
                }
                if !row[next] {
                    return None;
                }
            }
        }

        Some(Some(next))
    }
}

pub struct And<A, B> {
    pub left: A,
    pub right: B,
}

impl<A, B> And<A, B> {
    pub fn new(left: A, right: B) -> Self {
        Self { left, right }
    }
}

impl<A, B> WordAcceptor for And<A, B>
where
    A: WordAcceptor,
    B: WordAcceptor,
{
    type State = (A::State, B::State);

    fn start(&self) -> Self::State {
        (self.left.start(), self.right.start())
    }

    fn step(&self, state: &Self::State, next: usize) -> Option<Self::State> {
        let left = self.left.step(&state.0, next)?;
        let right = self.right.step(&state.1, next)?;
        Some((left, right))
    }

    fn is_accepting(&self, state: &Self::State, depth: usize) -> bool {
        self.left.is_accepting(&state.0, depth) && self.right.is_accepting(&state.1, depth)
    }
}
