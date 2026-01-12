use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use mpl_ir::{parse_sexpr, Expr};
use mpl_symbol::space::{Alphabet, ChannelId};

use crate::spec::common::{SpecAlphabet, SpecChannel};
use crate::ExperimentError;

pub(crate) type AlphabetBuildParts = (Alphabet, BTreeMap<String, usize>);

pub(crate) fn build_alphabet_from_spec(
    name: String,
    spec: &SpecAlphabet,
) -> Result<AlphabetBuildParts, ExperimentError> {
    if spec.letters.is_empty() {
        return Err(ExperimentError::InvalidConfig(
            "alphabet letters must be non-empty".to_string(),
        ));
    }

    let mut letters = Vec::with_capacity(spec.letters.len());
    let mut names = Vec::with_capacity(spec.letters.len());
    let mut name_to_idx: BTreeMap<String, usize> = BTreeMap::new();
    let mut channels: Vec<Option<ChannelId>> = Vec::with_capacity(spec.letters.len());

    for (idx, letter) in spec.letters.iter().enumerate() {
        if name_to_idx.insert(letter.name.clone(), idx).is_some() {
            return Err(ExperimentError::InvalidConfig(format!(
                "duplicate letter name: {}",
                letter.name
            )));
        }
        let expr = parse_sexpr(&letter.expr).map_err(|err| {
            ExperimentError::InvalidConfig(format!("letter '{}' parse error: {}", letter.name, err))
        })?;
        letters.push(expr.normalize());
        names.push(letter.name.clone());
        channels.push(letter.channel.as_ref().map(channel_id_from_spec));
    }

    let alphabet = Alphabet::new_with_channels(name, letters, names, channels);

    Ok((alphabet, name_to_idx))
}

pub fn toy_alphabet_xy() -> Alphabet {
    Alphabet {
        name: "toy_xy".to_string(),
        letters: vec![var("x"), var("y")],
        letter_names: vec!["x".to_string(), "y".to_string()],
        channels: vec![None; 2],
    }
}

pub fn toy_alphabet_xyz() -> Alphabet {
    Alphabet {
        name: "toy_xyz".to_string(),
        letters: vec![var("x"), var("y"), var("z")],
        letter_names: vec!["x".to_string(), "y".to_string(), "z".to_string()],
        channels: vec![None; 3],
    }
}

pub fn alphabet_from_file(path: &Path) -> Result<Alphabet, ExperimentError> {
    let content = fs::read_to_string(path)?;
    let mut letters = Vec::new();
    let mut names = Vec::new();

    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (name_opt, expr_str) = split_name_expr(trimmed);
        if expr_str.is_empty() {
            return Err(ExperimentError::InvalidConfig(format!(
                "empty expression at line {} in {}",
                idx + 1,
                path.display()
            )));
        }
        let expr = parse_sexpr(expr_str).map_err(|err| {
            ExperimentError::InvalidConfig(format!(
                "alphabet parse error at line {}: {}",
                idx + 1,
                err
            ))
        })?;
        let expr = expr.normalize();
        let name = match name_opt {
            Some(name) if !name.is_empty() => name,
            _ => expr.to_canonical_string(),
        };
        letters.push(expr);
        names.push(name);
    }

    if letters.is_empty() {
        return Err(ExperimentError::InvalidConfig(format!(
            "alphabet file '{}' has no letters",
            path.display()
        )));
    }

    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("alphabet")
        .to_string();

    let channels = vec![None; letters.len()];
    Ok(Alphabet {
        name,
        letters,
        letter_names: names,
        channels,
    })
}

pub(crate) fn normalize_inputs(
    alphabet: &Alphabet,
    constraints: &mpl_symbol::space::WordConstraints,
) -> (Alphabet, mpl_symbol::space::WordConstraints) {
    let letters: Vec<Expr> = alphabet.letters.iter().map(|e| e.normalize()).collect();
    let names = if alphabet.letter_names.len() == letters.len() {
        alphabet.letter_names.clone()
    } else {
        letters
            .iter()
            .map(|expr| expr.to_canonical_string())
            .collect()
    };

    let mut channels = alphabet.channels.clone();
    if channels.len() != letters.len() {
        channels.resize(letters.len(), None);
    }

    (
        Alphabet {
            name: alphabet.name.clone(),
            letters,
            letter_names: names,
            channels,
        },
        constraints.clone(),
    )
}

pub(crate) fn collect_vars_from_letters(letters: &[Expr]) -> Vec<String> {
    let mut vars = BTreeSet::new();
    for letter in letters {
        collect_vars(letter, &mut vars);
    }
    vars.into_iter().collect()
}

fn collect_vars(expr: &Expr, vars: &mut BTreeSet<String>) {
    match expr {
        Expr::Var(name) => {
            vars.insert(name.clone());
        }
        Expr::Add(children) | Expr::Mul(children) => {
            for child in children {
                collect_vars(child, vars);
            }
        }
        Expr::Neg(inner) => collect_vars(inner, vars),
        Expr::Pow(base, _) => collect_vars(base, vars),
        Expr::Rational(_) => {}
        Expr::Log(_) | Expr::Li2(_) => {}
    }
}

pub(crate) fn letter_display_names(alpha: &Alphabet) -> Vec<String> {
    if alpha.letter_names.len() == alpha.letters.len() {
        return alpha.letter_names.clone();
    }
    alpha
        .letters
        .iter()
        .map(|expr| expr.normalize().to_canonical_string())
        .collect()
}

fn var(name: &str) -> Expr {
    Expr::Var(name.to_string()).normalize()
}

fn split_name_expr(line: &str) -> (Option<String>, &str) {
    if let Some((name, expr)) = line.split_once('=') {
        return (Some(name.trim().to_string()), expr.trim());
    }
    if let Some((name, expr)) = line.split_once(':') {
        return (Some(name.trim().to_string()), expr.trim());
    }
    (None, line)
}

fn channel_id_from_spec(channel: &SpecChannel) -> ChannelId {
    match channel {
        SpecChannel::Int(value) => ChannelId::Numeric(*value),
        SpecChannel::Text(value) => ChannelId::from_name(value),
    }
}
