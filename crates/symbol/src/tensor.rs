use std::collections::BTreeMap;
use std::fmt;

use num_rational::Rational64;
use num_traits::Zero;

use crate::error::SymbolError;
use crate::hopf::Coproduct;
use crate::word::{shuffle_count_bounded, Word};

pub type Coeff = Rational64;

#[derive(Clone, Debug, Default)]
pub struct ShuffleFuel {
    remaining: Option<u64>,
}

impl ShuffleFuel {
    pub fn new(fuel: u64) -> Self {
        Self {
            remaining: Some(fuel),
        }
    }

    pub fn unlimited() -> Self {
        Self { remaining: None }
    }

    pub fn remaining(&self) -> Option<u64> {
        self.remaining
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

    pub fn shuffle_mul(
        &self,
        other: &Symbol,
        fuel: &mut ShuffleFuel,
    ) -> Result<Symbol, SymbolError> {
        if self.is_zero() || other.is_zero() {
            return Ok(Symbol::zero());
        }

        let mut out = Symbol::zero();
        for (left_word, left_coeff) in &self.terms {
            for (right_word, right_coeff) in &other.terms {
                if left_coeff.is_zero() || right_coeff.is_zero() {
                    continue;
                }
                reserve_shuffle_fuel(fuel, left_word.len(), right_word.len())?;
                let coeff = *left_coeff * *right_coeff;
                for shuffled in left_word.shuffle(right_word) {
                    out.add_term(shuffled, coeff);
                }
            }
        }
        Ok(out)
    }

    pub fn shuffle_pow(&self, exp: u32, fuel: &mut ShuffleFuel) -> Result<Symbol, SymbolError> {
        if exp == 0 || self.is_zero() {
            return Ok(Symbol::zero());
        }
        if exp == 1 {
            return Ok(self.clone());
        }
        let mut out = self.clone();
        for _ in 1..exp {
            out = out.shuffle_mul(self, fuel)?;
        }
        Ok(out)
    }

    pub fn deconcat(&self) -> Coproduct {
        let mut out = Coproduct::zero();
        for (word, coeff) in &self.terms {
            if coeff.is_zero() {
                continue;
            }
            for (left, right) in word.deconcat_splits() {
                out.add_term(left, right, *coeff);
            }
        }
        out
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

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_zero() {
            return write!(f, "0");
        }
        let mut first = true;
        for (word, coeff) in &self.terms {
            if !first {
                write!(f, " + ")?;
            }
            first = false;
            format_coeff(f, coeff)?;
            write!(f, "*{}", word)?;
        }
        Ok(())
    }
}

fn format_coeff(f: &mut fmt::Formatter<'_>, value: &Coeff) -> fmt::Result {
    let numer = *value.numer();
    let denom = *value.denom();
    if denom == 1 {
        write!(f, "{numer}")
    } else {
        write!(f, "{numer}/{denom}")
    }
}

fn reserve_shuffle_fuel(
    fuel: &mut ShuffleFuel,
    left_len: usize,
    right_len: usize,
) -> Result<(), SymbolError> {
    let remaining = match fuel.remaining {
        Some(value) => value,
        None => return Ok(()),
    };
    let count = match shuffle_count_bounded(left_len, right_len, remaining) {
        Some(value) => value,
        None => return Err(SymbolError::FuelExhausted),
    };
    fuel.remaining = Some(remaining - count);
    Ok(())
}
