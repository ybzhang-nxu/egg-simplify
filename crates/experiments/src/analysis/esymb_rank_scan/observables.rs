use std::collections::{BTreeMap, BTreeSet};

use mpl_symbol::Coeff;
use num_traits::Zero;

use crate::analysis::esymb_rank_scan::family::{
    format_letters_compact, FamilyType, SequenceSource, SequenceSpec,
};
use crate::analysis::esymb_rank_scan::rank::rank_matrix_mod_p;
use crate::output::csv::CsvWriter;
use crate::ExperimentError;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PairKey {
    prefix: Vec<String>,
    suffix: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct MatrixRankRow {
    pub loop_index: usize,
    pub prefix_len: usize,
    pub suffix_len: usize,
    pub nrows: usize,
    pub ncols: usize,
    pub rank_mod_p: usize,
}

#[derive(Clone, Debug)]
pub struct MarginalCollector {
    loops: Vec<usize>,
    prefix_len: Option<usize>,
    suffix_len: Option<usize>,
    collect_prefix: bool,
    collect_suffix: bool,
    collect_pair: bool,
    only_observed: bool,
    alphabet_set: BTreeSet<String>,
    alphabet_project: bool,
    prefix_keys: Vec<Vec<String>>,
    suffix_keys: Vec<Vec<String>>,
    pair_keys: Vec<PairKey>,
    prefix_values: BTreeMap<Vec<String>, Vec<Coeff>>,
    suffix_values: BTreeMap<Vec<String>, Vec<Coeff>>,
    pair_values: BTreeMap<PairKey, Vec<Coeff>>,
    observed_prefix: BTreeSet<Vec<String>>,
    observed_suffix: BTreeSet<Vec<String>>,
    observed_pairs: BTreeSet<PairKey>,
    totals: Vec<Coeff>,
}

pub struct MarginalCollectorConfig<'a> {
    pub loops: &'a [usize],
    pub letters: &'a [String],
    pub prefix_len: Option<usize>,
    pub suffix_len: Option<usize>,
    pub collect_prefix: bool,
    pub collect_suffix: bool,
    pub collect_pair: bool,
    pub only_observed: bool,
    pub alphabet_project: bool,
}

impl MarginalCollector {
    pub fn new(cfg: MarginalCollectorConfig) -> Self {
        let mut prefix_keys = Vec::new();
        let mut suffix_keys = Vec::new();
        let mut pair_keys = Vec::new();
        if cfg.collect_prefix || cfg.collect_pair {
            if let Some(r) = cfg.prefix_len {
                prefix_keys = enumerate_words(cfg.letters, r);
            }
        }
        if cfg.collect_suffix || cfg.collect_pair {
            if let Some(k) = cfg.suffix_len {
                suffix_keys = enumerate_words(cfg.letters, k);
            }
        }
        if cfg.collect_pair {
            for prefix in &prefix_keys {
                for suffix in &suffix_keys {
                    pair_keys.push(PairKey {
                        prefix: prefix.clone(),
                        suffix: suffix.clone(),
                    });
                }
            }
        }

        let loops_len = cfg.loops.len();
        let mut prefix_values = BTreeMap::new();
        let mut suffix_values = BTreeMap::new();
        let mut pair_values = BTreeMap::new();
        if !cfg.only_observed {
            if cfg.collect_prefix {
                for key in &prefix_keys {
                    prefix_values.insert(key.clone(), vec![Coeff::zero(); loops_len]);
                }
            }
            if cfg.collect_suffix {
                for key in &suffix_keys {
                    suffix_values.insert(key.clone(), vec![Coeff::zero(); loops_len]);
                }
            }
            if cfg.collect_pair {
                for key in &pair_keys {
                    pair_values.insert(key.clone(), vec![Coeff::zero(); loops_len]);
                }
            }
        }

        let mut alphabet_set = BTreeSet::new();
        for letter in cfg.letters {
            alphabet_set.insert(letter.clone());
        }

        Self {
            loops: cfg.loops.to_vec(),
            prefix_len: cfg.prefix_len,
            suffix_len: cfg.suffix_len,
            collect_prefix: cfg.collect_prefix,
            collect_suffix: cfg.collect_suffix,
            collect_pair: cfg.collect_pair,
            only_observed: cfg.only_observed,
            alphabet_set,
            alphabet_project: cfg.alphabet_project,
            prefix_keys,
            suffix_keys,
            pair_keys,
            prefix_values,
            suffix_values,
            pair_values,
            observed_prefix: BTreeSet::new(),
            observed_suffix: BTreeSet::new(),
            observed_pairs: BTreeSet::new(),
            totals: vec![Coeff::zero(); loops_len],
        }
    }

    pub fn observe_term(
        &mut self,
        loop_idx: usize,
        loop_value: usize,
        word: &[String],
        coeff: Coeff,
    ) -> Result<(), ExperimentError> {
        if !self.collect_prefix && !self.collect_suffix && !self.collect_pair {
            return Ok(());
        }
        if word.len() != 2 * loop_value {
            return Err(ExperimentError::InvalidConfig(format!(
                "word length mismatch for L={loop_value}: expected {}, got {}",
                2 * loop_value,
                word.len()
            )));
        }
        if !self.word_allowed(word)? {
            return Ok(());
        }

        if loop_idx >= self.totals.len() {
            return Err(ExperimentError::InvalidConfig(format!(
                "loop index out of range: {loop_idx}"
            )));
        }
        self.totals[loop_idx] += coeff;

        if self.collect_prefix {
            let r = self.prefix_len.unwrap_or(0);
            let prefix = word[..r].to_vec();
            bump_map(
                &mut self.prefix_values,
                prefix.clone(),
                loop_idx,
                coeff,
                self.loops.len(),
            );
            self.observed_prefix.insert(prefix);
        }
        if self.collect_suffix {
            let k = self.suffix_len.unwrap_or(0);
            let suffix = word[word.len().saturating_sub(k)..].to_vec();
            bump_map(
                &mut self.suffix_values,
                suffix.clone(),
                loop_idx,
                coeff,
                self.loops.len(),
            );
            self.observed_suffix.insert(suffix);
        }
        if self.collect_pair {
            let r = self.prefix_len.unwrap_or(0);
            let k = self.suffix_len.unwrap_or(0);
            let prefix = word[..r].to_vec();
            let suffix = word[word.len().saturating_sub(k)..].to_vec();
            let key = PairKey { prefix, suffix };
            bump_map(
                &mut self.pair_values,
                key.clone(),
                loop_idx,
                coeff,
                self.loops.len(),
            );
            self.observed_pairs.insert(key);
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), ExperimentError> {
        let loops_len = self.loops.len();
        if loops_len != self.totals.len() {
            return Err(ExperimentError::InvalidConfig(
                "totals length mismatch".to_string(),
            ));
        }
        let prefix_keys = self.prefix_keys_filtered();
        let suffix_keys = self.suffix_keys_filtered();
        for loop_idx in 0..loops_len {
            let total = self.totals[loop_idx];
            if self.collect_suffix {
                let mut sum = Coeff::zero();
                for &key in suffix_keys.iter() {
                    if let Some(values) = self.suffix_values.get(key) {
                        sum += values[loop_idx];
                    }
                }
                if sum != total {
                    return Err(ExperimentError::InvalidConfig(format!(
                        "suffix marginal mismatch at L={}: sum {} != total {}",
                        self.loops[loop_idx],
                        format_coeff(&sum),
                        format_coeff(&total)
                    )));
                }
            }
            if self.collect_prefix {
                let mut sum = Coeff::zero();
                for &key in prefix_keys.iter() {
                    if let Some(values) = self.prefix_values.get(key) {
                        sum += values[loop_idx];
                    }
                }
                if sum != total {
                    return Err(ExperimentError::InvalidConfig(format!(
                        "prefix marginal mismatch at L={}: sum {} != total {}",
                        self.loops[loop_idx],
                        format_coeff(&sum),
                        format_coeff(&total)
                    )));
                }
            }
        }

        if self.collect_pair && self.collect_prefix {
            for loop_idx in 0..loops_len {
                for &prefix in prefix_keys.iter() {
                    let mut sum = Coeff::zero();
                    for &suffix in suffix_keys.iter() {
                        let key = PairKey {
                            prefix: prefix.to_vec(),
                            suffix: suffix.to_vec(),
                        };
                        if let Some(values) = self.pair_values.get(&key) {
                            sum += values[loop_idx];
                        }
                    }
                    let expected = self
                        .prefix_values
                        .get(prefix)
                        .map(|values| values[loop_idx])
                        .unwrap_or_else(Coeff::zero);
                    if sum != expected {
                        return Err(ExperimentError::InvalidConfig(format!(
                            "prefix-suffix mismatch at L={} for prefix={}: {} != {}",
                            self.loops[loop_idx],
                            format_letters_compact(prefix),
                            format_coeff(&sum),
                            format_coeff(&expected)
                        )));
                    }
                }
            }
        }

        if self.collect_pair && self.collect_suffix {
            for loop_idx in 0..loops_len {
                for &suffix in suffix_keys.iter() {
                    let mut sum = Coeff::zero();
                    for &prefix in prefix_keys.iter() {
                        let key = PairKey {
                            prefix: prefix.to_vec(),
                            suffix: suffix.to_vec(),
                        };
                        if let Some(values) = self.pair_values.get(&key) {
                            sum += values[loop_idx];
                        }
                    }
                    let expected = self
                        .suffix_values
                        .get(suffix)
                        .map(|values| values[loop_idx])
                        .unwrap_or_else(Coeff::zero);
                    if sum != expected {
                        return Err(ExperimentError::InvalidConfig(format!(
                            "prefix-suffix mismatch at L={} for suffix={}: {} != {}",
                            self.loops[loop_idx],
                            format_letters_compact(suffix),
                            format_coeff(&sum),
                            format_coeff(&expected)
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn sequences_and_values(
        &self,
        include_prefix: bool,
        include_suffix: bool,
        include_pair: bool,
    ) -> (Vec<SequenceSpec>, Vec<Vec<Coeff>>) {
        let mut sequences = Vec::new();
        let mut values = Vec::new();
        if include_prefix {
            let r = self.prefix_len.unwrap_or(0);
            for key in self.prefix_keys_filtered() {
                let params = vec![
                    format!("r={r}"),
                    format!("p={}", format_letters_compact(key)),
                ];
                sequences.push(SequenceSpec {
                    family: FamilyType::Prefix,
                    params,
                    source: SequenceSource::Prefix {
                        prefix: key.to_vec(),
                    },
                });
                values.push(self.value_or_zero(&self.prefix_values, key));
            }
        }
        if include_suffix {
            let k = self.suffix_len.unwrap_or(0);
            for key in self.suffix_keys_filtered() {
                let params = vec![
                    format!("k={k}"),
                    format!("s={}", format_letters_compact(key)),
                ];
                sequences.push(SequenceSpec {
                    family: FamilyType::Suffix,
                    params,
                    source: SequenceSource::Suffix {
                        suffix: key.to_vec(),
                    },
                });
                values.push(self.value_or_zero(&self.suffix_values, key));
            }
        }
        if include_pair {
            let r = self.prefix_len.unwrap_or(0);
            let k = self.suffix_len.unwrap_or(0);
            for key in self.pair_keys_filtered() {
                let params = vec![
                    format!("r={r}"),
                    format!("k={k}"),
                    format!("u={}", format_letters_compact(&key.prefix)),
                    format!("v={}", format_letters_compact(&key.suffix)),
                ];
                sequences.push(SequenceSpec {
                    family: FamilyType::PrefixSuffix,
                    params,
                    source: SequenceSource::PrefixSuffix {
                        prefix: key.prefix.clone(),
                        suffix: key.suffix.clone(),
                    },
                });
                values.push(self.pair_value_or_zero(key));
            }
        }
        (sequences, values)
    }

    pub fn matrix_rank_rows(&self, primes: &[i64]) -> Result<Vec<MatrixRankRow>, ExperimentError> {
        if !self.collect_pair {
            return Ok(Vec::new());
        }
        let r = self.prefix_len.unwrap_or(0);
        let k = self.suffix_len.unwrap_or(0);
        let prefix_keys = self.prefix_keys_filtered();
        let suffix_keys = self.suffix_keys_filtered();
        let nrows = prefix_keys.len();
        let ncols = suffix_keys.len();
        let mut rows = Vec::with_capacity(self.loops.len());
        for (loop_idx, loop_value) in self.loops.iter().copied().enumerate() {
            if nrows == 0 || ncols == 0 {
                rows.push(MatrixRankRow {
                    loop_index: loop_value,
                    prefix_len: r,
                    suffix_len: k,
                    nrows,
                    ncols,
                    rank_mod_p: 0,
                });
                continue;
            }
            let mut matrix = vec![vec![Coeff::zero(); ncols]; nrows];
            for (i, prefix) in prefix_keys.iter().enumerate() {
                for (j, suffix) in suffix_keys.iter().enumerate() {
                    let key = PairKey {
                        prefix: (*prefix).clone(),
                        suffix: (*suffix).clone(),
                    };
                    let value = self
                        .pair_values
                        .get(&key)
                        .map(|values| values[loop_idx])
                        .unwrap_or_else(Coeff::zero);
                    matrix[i][j] = value;
                }
            }
            let rank_mod_p = rank_matrix_mod_p(&matrix, primes)?;
            rows.push(MatrixRankRow {
                loop_index: loop_value,
                prefix_len: r,
                suffix_len: k,
                nrows,
                ncols,
                rank_mod_p,
            });
        }
        Ok(rows)
    }

    fn word_allowed(&self, word: &[String]) -> Result<bool, ExperimentError> {
        if self.alphabet_set.is_empty() {
            return Ok(true);
        }
        for token in word {
            if !self.alphabet_set.contains(token) {
                if self.alphabet_project {
                    return Ok(false);
                }
                return Err(ExperimentError::InvalidConfig(format!(
                    "unknown alphabet token: {token}"
                )));
            }
        }
        Ok(true)
    }

    fn prefix_keys_filtered(&self) -> Vec<&Vec<String>> {
        self.prefix_keys
            .iter()
            .filter(|key| !self.only_observed || self.observed_prefix.contains(*key))
            .collect()
    }

    fn suffix_keys_filtered(&self) -> Vec<&Vec<String>> {
        self.suffix_keys
            .iter()
            .filter(|key| !self.only_observed || self.observed_suffix.contains(*key))
            .collect()
    }

    fn pair_keys_filtered(&self) -> Vec<&PairKey> {
        self.pair_keys
            .iter()
            .filter(|key| !self.only_observed || self.observed_pairs.contains(*key))
            .collect()
    }

    fn value_or_zero(
        &self,
        map: &BTreeMap<Vec<String>, Vec<Coeff>>,
        key: &Vec<String>,
    ) -> Vec<Coeff> {
        map.get(key)
            .cloned()
            .unwrap_or_else(|| vec![Coeff::zero(); self.loops.len()])
    }

    fn pair_value_or_zero(&self, key: &PairKey) -> Vec<Coeff> {
        self.pair_values
            .get(key)
            .cloned()
            .unwrap_or_else(|| vec![Coeff::zero(); self.loops.len()])
    }
}

pub fn render_marginals_observables_csv(
    sequences: &[SequenceSpec],
    values: &[Vec<Coeff>],
    loops: &[usize],
) -> String {
    let mut writer = CsvWriter::new();
    let mut header = vec!["family".to_string(), "params".to_string()];
    for loop_index in loops {
        header.push(format!("cL{loop_index}"));
    }
    writer.push_record(header);
    for (spec, row) in sequences.iter().zip(values.iter()) {
        if !spec.is_marginal() {
            continue;
        }
        let mut fields = Vec::new();
        fields.push(spec.family.as_str().to_string());
        fields.push(spec.param_string());
        for value in row {
            fields.push(format_coeff(value));
        }
        writer.push_record(fields);
    }
    writer.into_string()
}

pub fn render_marginals_matrix_rank_csv(rows: &[MatrixRankRow]) -> String {
    let mut writer = CsvWriter::new();
    writer.push_record([
        "loop",
        "prefix_len",
        "suffix_len",
        "nrows",
        "ncols",
        "rank_mod_p",
    ]);
    for row in rows {
        writer.push_record([
            row.loop_index.to_string(),
            row.prefix_len.to_string(),
            row.suffix_len.to_string(),
            row.nrows.to_string(),
            row.ncols.to_string(),
            row.rank_mod_p.to_string(),
        ]);
    }
    writer.into_string()
}

fn bump_map<K: Ord + Clone>(
    map: &mut BTreeMap<K, Vec<Coeff>>,
    key: K,
    loop_idx: usize,
    coeff: Coeff,
    loops_len: usize,
) {
    let entry = map.entry(key).or_default();
    if entry.len() < loops_len {
        entry.resize(loops_len, Coeff::zero());
    }
    entry[loop_idx] += coeff;
}

fn enumerate_words(letters: &[String], len: usize) -> Vec<Vec<String>> {
    if len == 0 {
        return vec![Vec::new()];
    }
    if letters.is_empty() {
        return Vec::new();
    }
    let base = letters.len();
    let mut total = 1usize;
    for _ in 0..len {
        total = total.saturating_mul(base);
    }
    let mut out = Vec::with_capacity(total);
    let mut indices = vec![0usize; len];
    loop {
        let mut word = Vec::with_capacity(len);
        for &idx in &indices {
            word.push(letters[idx].clone());
        }
        out.push(word);
        let mut pos = len;
        while pos > 0 {
            pos -= 1;
            indices[pos] += 1;
            if indices[pos] < base {
                break;
            }
            indices[pos] = 0;
        }
        if pos == 0 && indices[0] == 0 {
            break;
        }
    }
    out
}

fn format_coeff(value: &Coeff) -> String {
    let numer = *value.numer();
    let denom = *value.denom();
    if denom == 1 {
        numer.to_string()
    } else {
        format!("{numer}/{denom}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_ranks_simple_matrix() {
        let loops = vec![1];
        let letters = vec!["a".to_string(), "b".to_string()];
        let mut collector = MarginalCollector::new(MarginalCollectorConfig {
            loops: &loops,
            letters: &letters,
            prefix_len: Some(1),
            suffix_len: Some(1),
            collect_prefix: true,
            collect_suffix: true,
            collect_pair: true,
            only_observed: false,
            alphabet_project: false,
        });
        collector
            .observe_term(
                0,
                1,
                &["a".to_string(), "b".to_string()],
                Coeff::from_integer(1),
            )
            .expect("observe");
        collector
            .observe_term(
                0,
                1,
                &["b".to_string(), "a".to_string()],
                Coeff::from_integer(2),
            )
            .expect("observe");

        collector.validate().expect("validate");

        let rows = collector.matrix_rank_rows(&[101]).expect("rank rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].rank_mod_p, 2);
    }
}
