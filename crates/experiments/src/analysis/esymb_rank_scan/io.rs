use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use mpl_symbol::Coeff;
use serde::Deserialize;

use crate::ExperimentError;

#[derive(Clone, Debug)]
pub struct EsymbMeta {
    pub name: String,
    pub loop_index: usize,
    pub merged_terms: usize,
}

#[derive(Clone, Debug)]
pub struct EsymbTerm {
    pub word: Vec<String>,
    pub coeff: Coeff,
}

#[derive(Deserialize)]
struct MetaLine {
    #[serde(rename = "_meta")]
    meta: MetaContent,
}

#[derive(Deserialize)]
struct MetaContent {
    name: String,
    #[serde(rename = "loop")]
    loop_index: usize,
    #[serde(default)]
    merged_terms: Option<usize>,
}

#[derive(Deserialize)]
struct TermLine {
    word: Vec<String>,
    coeff: String,
}

pub fn read_esymb_jsonl_meta(path: &Path) -> Result<EsymbMeta, ExperimentError> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut first = String::new();
    if reader.read_line(&mut first)? == 0 {
        return Err(ExperimentError::InvalidConfig(format!(
            "empty jsonl file: {}",
            path.display()
        )));
    }
    let line = first.trim();
    let meta_line: MetaLine = serde_json::from_str(line).map_err(|err| {
        ExperimentError::InvalidConfig(format!("invalid meta line in {}: {err}", path.display()))
    })?;
    Ok(EsymbMeta {
        name: meta_line.meta.name,
        loop_index: meta_line.meta.loop_index,
        merged_terms: meta_line.meta.merged_terms.unwrap_or(0),
    })
}

pub struct EsymbJsonlReader {
    path: PathBuf,
    lines: std::io::Lines<BufReader<File>>,
}

impl EsymbJsonlReader {
    pub fn next_term(&mut self) -> Result<Option<EsymbTerm>, ExperimentError> {
        while let Some(line) = self.lines.next() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with("{\"_meta\"") {
                continue;
            }
            let term_line: TermLine = serde_json::from_str(trimmed).map_err(|err| {
                ExperimentError::InvalidConfig(format!(
                    "invalid term line in {}: {err}",
                    self.path.display()
                ))
            })?;
            let coeff = parse_coeff(&term_line.coeff)?;
            return Ok(Some(EsymbTerm {
                word: term_line.word,
                coeff,
            }));
        }
        Ok(None)
    }
}

pub fn stream_esymb_terms(path: &Path) -> Result<EsymbJsonlReader, ExperimentError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    Ok(EsymbJsonlReader {
        path: path.to_path_buf(),
        lines: reader.lines(),
    })
}

fn parse_coeff(text: &str) -> Result<Coeff, ExperimentError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Coeff::from_integer(0));
    }
    if let Some((num, denom)) = trimmed.split_once('/') {
        let n: i64 = num.trim().parse().map_err(|_| {
            ExperimentError::InvalidConfig(format!("invalid coeff numerator: {text}"))
        })?;
        let d: i64 = denom.trim().parse().map_err(|_| {
            ExperimentError::InvalidConfig(format!("invalid coeff denominator: {text}"))
        })?;
        if d == 0 {
            return Err(ExperimentError::InvalidConfig(format!(
                "zero denominator in coeff: {text}"
            )));
        }
        return Ok(Coeff::new(n, d));
    }
    let n: i64 = trimmed
        .parse()
        .map_err(|_| ExperimentError::InvalidConfig(format!("invalid coeff: {text}")))?;
    Ok(Coeff::from_integer(n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        path.push(format!("mpl_esymb_{name}_{stamp}.jsonl"));
        path
    }

    #[test]
    fn parse_meta_and_terms() {
        let path = temp_path("meta_terms");
        let content = r#"{"_meta":{"name":"Esymb","loop":1,"merged_terms":2}}
{"word":["a","b"],"coeff":"1/2"}
{"word":["b","a"],"coeff":"-3"}
"#;
        fs::write(&path, content).expect("write temp jsonl");

        let meta = read_esymb_jsonl_meta(&path).expect("meta");
        assert_eq!(meta.name, "Esymb");
        assert_eq!(meta.loop_index, 1);
        assert_eq!(meta.merged_terms, 2);

        let mut reader = stream_esymb_terms(&path).expect("reader");
        let first = reader.next_term().expect("term").expect("term");
        assert_eq!(first.word, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(first.coeff, Coeff::new(1, 2));
        let second = reader.next_term().expect("term").expect("term");
        assert_eq!(second.word, vec!["b".to_string(), "a".to_string()]);
        assert_eq!(second.coeff, Coeff::from_integer(-3));
        let none = reader.next_term().expect("term");
        assert!(none.is_none());

        let _ = fs::remove_file(&path);
    }
}
