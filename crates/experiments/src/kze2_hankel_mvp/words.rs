use crate::ExperimentError;

pub type LetterId = u8;
pub type Word = Vec<LetterId>;

/// Enumerate words by length, then lexicographic letter id within each length.
pub fn words_upto(max_len: usize, alphabet_size: usize) -> Vec<Word> {
    let mut out = Vec::new();
    out.push(Vec::new());
    for len in 1..=max_len {
        let mut current = Vec::with_capacity(len);
        build_words(len, alphabet_size, &mut current, &mut out);
    }
    out
}

pub fn for_each_word_len<F>(len: usize, alphabet_size: usize, mut func: F)
where
    F: FnMut(&[LetterId]),
{
    let mut buffer = vec![0u8; len];
    enumerate_len(0, len, alphabet_size, &mut buffer, &mut func);
}

pub fn count_words_exact(len: usize, alphabet_size: usize) -> Result<u64, ExperimentError> {
    let mut total: u128 = 1;
    for _ in 0..len {
        total = total.checked_mul(alphabet_size as u128).ok_or_else(|| {
            ExperimentError::InvalidConfig("holdout word count overflow".to_string())
        })?;
    }
    if total > u64::MAX as u128 {
        return Err(ExperimentError::InvalidConfig(
            "holdout word count overflow".to_string(),
        ));
    }
    Ok(total as u64)
}

fn build_words(len: usize, alphabet_size: usize, current: &mut Word, out: &mut Vec<Word>) {
    if current.len() == len {
        out.push(current.clone());
        return;
    }
    for letter in 0..alphabet_size {
        current.push(letter as LetterId);
        build_words(len, alphabet_size, current, out);
        current.pop();
    }
}

fn enumerate_len<F>(
    pos: usize,
    len: usize,
    alphabet_size: usize,
    buffer: &mut [LetterId],
    func: &mut F,
) where
    F: FnMut(&[LetterId]),
{
    if pos == len {
        func(buffer);
        return;
    }
    for letter in 0..alphabet_size {
        buffer[pos] = letter as LetterId;
        enumerate_len(pos + 1, len, alphabet_size, buffer, func);
    }
}
