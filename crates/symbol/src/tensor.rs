use std::cmp::Ordering;
use std::collections::BTreeMap;

use mpl_ir::Expr;
use num_rational::Rational64;
use num_traits::Zero;

pub type Coeff = Rational64;

#[derive(Clone, Debug)]
pub struct Word(pub Vec<Expr>);

impl Word {
    pub fn letters(&self) -> &[Expr] {
        &self.0
    }
}

impl PartialEq for Word {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Word {}

impl PartialOrd for Word {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Word {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut left = self.0.iter().map(|expr| expr.to_canonical_string());
        let mut right = other.0.iter().map(|expr| expr.to_canonical_string());
        loop {
            match (left.next(), right.next()) {
                (Some(a), Some(b)) => {
                    let order = a.cmp(&b);
                    if order != Ordering::Equal {
                        return order;
                    }
                }
                (None, Some(_)) => return Ordering::Less,
                (Some(_), None) => return Ordering::Greater,
                (None, None) => return Ordering::Equal,
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    terms: BTreeMap<Word, Coeff>,
}

impl Symbol {
    /// Build a symbol from explicit terms, combining duplicates and dropping zeros.
    pub fn from_terms<I>(terms: I) -> Self
    where
        I: IntoIterator<Item = (Word, Coeff)>,
    {
        let mut out = Self::zero();
        for (word, coeff) in terms {
            out.add_term(word, coeff);
        }
        out
    }

    pub fn zero() -> Self {
        Self {
            terms: BTreeMap::new(),
        }
    }

    pub fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }

    pub fn terms(&self) -> impl Iterator<Item = (&Word, &Coeff)> {
        self.terms.iter()
    }

    pub(crate) fn add_term(&mut self, word: Word, coeff: Coeff) {
        if coeff.is_zero() {
            return;
        }
        use std::collections::btree_map::Entry;
        match self.terms.entry(word) {
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

    pub(crate) fn add_assign(&mut self, other: Symbol) {
        for (word, coeff) in other.terms {
            self.add_term(word, coeff);
        }
    }

    pub(crate) fn scale(&mut self, coeff: Coeff) {
        if coeff.is_zero() {
            self.terms.clear();
            return;
        }
        for value in self.terms.values_mut() {
            *value *= coeff;
        }
    }
}
