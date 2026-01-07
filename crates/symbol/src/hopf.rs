use std::collections::BTreeMap;

use num_traits::Zero;

use crate::tensor::Coeff;
use crate::word::Word;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Coproduct {
    terms: BTreeMap<(Word, Word), Coeff>,
}

impl Coproduct {
    pub fn zero() -> Self {
        Self {
            terms: BTreeMap::new(),
        }
    }

    pub fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }

    pub fn terms(&self) -> impl Iterator<Item = (&(Word, Word), &Coeff)> {
        self.terms.iter()
    }

    pub(crate) fn add_term(&mut self, left: Word, right: Word, coeff: Coeff) {
        if coeff.is_zero() {
            return;
        }
        use std::collections::btree_map::Entry;
        match self.terms.entry((left, right)) {
            Entry::Vacant(entry) => {
                entry.insert(coeff);
            }
            Entry::Occupied(mut entry) => {
                let updated = *entry.get() + coeff;
                if updated.is_zero() {
                    entry.remove();
                } else {
                    entry.insert(updated);
                }
            }
        }
    }
}
