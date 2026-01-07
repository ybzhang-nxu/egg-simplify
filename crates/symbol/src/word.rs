use std::cmp::Ordering;
use std::fmt;

use mpl_ir::Expr;

#[derive(Clone, Debug)]
pub struct Word(pub Vec<Expr>);

impl Word {
    pub fn letters(&self) -> &[Expr] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn shuffle(&self, other: &Word) -> Vec<Word> {
        let mut out = Vec::new();
        let mut prefix = Vec::with_capacity(self.len() + other.len());
        shuffle_rec(self.letters(), other.letters(), &mut prefix, &mut out);
        out
    }

    pub fn deconcat_splits(&self) -> Vec<(Word, Word)> {
        let mut out = Vec::with_capacity(self.len() + 1);
        for split in 0..=self.len() {
            let left = Word(self.0[..split].to_vec());
            let right = Word(self.0[split..].to_vec());
            out.push((left, right));
        }
        out
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

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        for (idx, letter) in self.0.iter().enumerate() {
            if idx > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", letter.to_canonical_string())?;
        }
        write!(f, "]")
    }
}

pub fn shuffle_count_bounded(left_len: usize, right_len: usize, limit: u64) -> Option<u64> {
    if limit == 0 {
        return None;
    }
    let n = left_len + right_len;
    if n == 0 {
        return Some(1);
    }
    let k = left_len.min(right_len);
    if k == 0 {
        return Some(1);
    }
    let mut result: u128 = 1;
    for i in 1..=k {
        let numerator = (n - k + i) as u128;
        result = result.checked_mul(numerator)?;
        result /= i as u128;
        if result > limit as u128 {
            return None;
        }
    }
    Some(result as u64)
}

fn shuffle_rec(left: &[Expr], right: &[Expr], prefix: &mut Vec<Expr>, out: &mut Vec<Word>) {
    if left.is_empty() {
        let mut word = prefix.clone();
        word.extend_from_slice(right);
        out.push(Word(word));
        return;
    }
    if right.is_empty() {
        let mut word = prefix.clone();
        word.extend_from_slice(left);
        out.push(Word(word));
        return;
    }

    prefix.push(left[0].clone());
    shuffle_rec(&left[1..], right, prefix, out);
    prefix.pop();

    prefix.push(right[0].clone());
    shuffle_rec(left, &right[1..], prefix, out);
    prefix.pop();
}
