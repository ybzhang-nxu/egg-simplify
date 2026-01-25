use std::collections::{BTreeMap, BTreeSet};

use mpl_experiments::{
    pentaladder_alphabet, symbol_psi, symbol_psi1_golden, symbol_psi2_blocks,
    symbol_psi2_golden, symbol_q_blocks_expanded, trace_psi2_origin_details,
    trace_psi2_origin_report, OriginKind, OriginTraceTerm,
};
use mpl_symbol::{Coeff, Symbol, Word};
use num_traits::Zero;

#[test]
fn psi1_matches_golden() {
    let sym = symbol_psi(1).expect("psi1");
    let golden = symbol_psi1_golden().expect("psi1 golden");
    assert_eq!(sym, golden);
}

#[test]
fn psi2_matches_golden() {
    let sym = symbol_psi(2).expect("psi2");
    let golden = symbol_psi2_golden().expect("psi2 golden");
    if sym != golden {
        let left = symbol_terms_map(&sym);
        let right = symbol_terms_map(&golden);
        let mut left_only = 0usize;
        let mut right_only = 0usize;
        let mut coeff_mismatch = 0usize;
        let mut left_only_terms = Vec::new();
        let mut right_only_terms = Vec::new();
        let mut mismatch_terms = Vec::new();
        for (word, coeff) in &left {
            match right.get(word) {
                None => {
                    left_only += 1;
                    left_only_terms.push((word.clone(), *coeff));
                }
                Some(other) => {
                    if other != coeff {
                        coeff_mismatch += 1;
                        mismatch_terms.push((word.clone(), *coeff, *other));
                    }
                }
            }
        }
        for word in right.keys() {
            if !left.contains_key(word) {
                right_only += 1;
                let coeff = *right.get(word).expect("right coeff");
                right_only_terms.push((word.clone(), coeff));
            }
        }
        left_only_terms.sort_by(|a, b| a.0.cmp(&b.0));
        right_only_terms.sort_by(|a, b| a.0.cmp(&b.0));
        mismatch_terms.sort_by(|a, b| a.0.cmp(&b.0));
        let left_only_dump = left_only_terms
            .iter()
            .map(|(word, coeff)| format!("{word}: {coeff}"))
            .collect::<Vec<_>>()
            .join("\n");
        let right_only_dump = right_only_terms
            .iter()
            .map(|(word, coeff)| format!("{word}: {coeff}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mismatch_dump = mismatch_terms
            .iter()
            .map(|(word, left_coeff, right_coeff)| {
                format!("{word}: left={left_coeff}, right={right_coeff}")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let block_dump = diff_psi2_blocks(&left_only_terms, &right_only_terms, &mismatch_terms);
        let origin_dump = diff_psi2_origins(&left_only_terms, &right_only_terms, &mismatch_terms);
        let block_extract_dump = diff_last_entry_blocks();
        panic!(
            "psi2 mismatch: left_only={left_only}, right_only={right_only}, coeff_mismatch={coeff_mismatch}, left_terms={}, right_terms={}\nleft_only:\n{left_only_dump}\nright_only:\n{right_only_dump}\ncoeff_mismatch:\n{mismatch_dump}\nblock_diff:\n{block_dump}\norigin_diff:\n{origin_dump}\nblock_extract:\n{block_extract_dump}",
            left.len(),
            right.len()
        );
    }
}

fn symbol_terms_map(sym: &Symbol) -> BTreeMap<Word, Coeff> {
    let mut out = BTreeMap::new();
    for (word, coeff) in sym.terms() {
        if coeff.is_zero() {
            continue;
        }
        out.insert(word.clone(), *coeff);
    }
    out
}

fn diff_psi2_blocks(
    left_only_terms: &[(Word, Coeff)],
    right_only_terms: &[(Word, Coeff)],
    mismatch_terms: &[(Word, Coeff, Coeff)],
) -> String {
    let blocks = symbol_psi2_blocks().expect("psi2 blocks");
    let e1 = symbol_terms_map(&blocks.e1);
    let e2 = symbol_terms_map(&blocks.e2);
    let e3 = symbol_terms_map(&blocks.e3);

    let mut out = Vec::new();
    out.push(format!(
        "block terms: e1={}, e2={}, e3={}",
        e1.len(),
        e2.len(),
        e3.len()
    ));
    for (label, terms) in [
        ("left_only", left_only_terms.iter().map(|(w, _)| w).collect::<Vec<_>>()),
        ("right_only", right_only_terms.iter().map(|(w, _)| w).collect::<Vec<_>>()),
        ("coeff_mismatch", mismatch_terms.iter().map(|(w, _, _)| w).collect::<Vec<_>>()),
    ] {
        if terms.is_empty() {
            continue;
        }
        out.push(format!("{label} block membership:"));
        for word in terms {
            let e1c = e1.get(word).cloned().unwrap_or_else(Coeff::zero);
            let e2c = e2.get(word).cloned().unwrap_or_else(Coeff::zero);
            let e3c = e3.get(word).cloned().unwrap_or_else(Coeff::zero);
            out.push(format!(
                "{word}: e1={e1c}, e2={e2c}, e3={e3c}"
            ));
        }
    }
    out.join("\n")
}

fn diff_psi2_origins(
    left_only_terms: &[(Word, Coeff)],
    right_only_terms: &[(Word, Coeff)],
    mismatch_terms: &[(Word, Coeff, Coeff)],
) -> String {
    let mut targets = BTreeSet::new();
    for (word, _) in left_only_terms {
        targets.insert(word.clone());
    }
    for (word, _) in right_only_terms {
        targets.insert(word.clone());
    }
    for (word, _, _) in mismatch_terms {
        targets.insert(word.clone());
    }
    if targets.is_empty() {
        return "no origin targets".to_string();
    }

    let report = trace_psi2_origin_report().expect("trace psi2 origins");
    let origin_map = build_origin_map(&report.terms);
    let detail_map = trace_psi2_origin_details(&targets).expect("trace psi2 origin details");
    let mut out = Vec::new();
    out.push(format!(
        "origin terms: raw={}, expanded={}",
        report.raw_terms, report.expanded_terms
    ));
    for word in targets {
        let entries = origin_map.get(&word);
        if let Some(entries) = entries {
            let mut parts = Vec::new();
            for (origin, coeff) in entries {
                parts.push(format!("{origin}={coeff}"));
            }
            out.push(format!("{word}: {}", parts.join(", ")));
        } else {
            out.push(format!("{word}: <no origin trace>"));
        }
        if let Some(details) = detail_map.get(&word) {
            for detail in details {
                out.push(format!(
                    "  detail origin={} coeff={} stage={} kernel={} source_last={} normalized_last={} last={} d={} diff={} atoms=[{}]",
                    detail.origin,
                    detail.coeff,
                    detail.stage,
                    detail.kernel,
                    detail.source_last.as_deref().unwrap_or("<none>"),
                    detail.normalized_last.as_deref().unwrap_or("<none>"),
                    detail.last.as_deref().unwrap_or("<none>"),
                    detail.d.as_deref().unwrap_or("<none>"),
                    detail.diff.as_deref().unwrap_or("<none>"),
                    detail.atoms.join(", "),
                ));
            }
        } else {
            out.push("  detail <none>".to_string());
        }
    }
    out.join("\n")
}

fn build_origin_map(
    trace: &[OriginTraceTerm],
) -> BTreeMap<Word, BTreeMap<OriginKind, Coeff>> {
    let mut map: BTreeMap<Word, BTreeMap<OriginKind, Coeff>> = BTreeMap::new();
    for term in trace {
        let entry = map.entry(term.word.clone()).or_default();
        let coeff = entry.entry(term.origin).or_insert_with(Coeff::zero);
        *coeff += term.coeff;
    }
    map
}

fn diff_last_entry_blocks() -> String {
    let alpha = pentaladder_alphabet();
    let mut name_map = BTreeMap::new();
    for (expr, name) in alpha.letters.iter().zip(alpha.letter_names.iter()) {
        name_map.insert(expr.to_canonical_string(), name.clone());
    }
    let sym = symbol_psi(2).expect("psi2");
    let buckets = bucket_prefix_by_last(&sym, &name_map);
    let p_u = buckets.get("u").cloned().unwrap_or_else(Symbol::zero);
    let p_1u = buckets.get("1-u").cloned().unwrap_or_else(Symbol::zero);
    let p_v = buckets.get("v").cloned().unwrap_or_else(Symbol::zero);
    let p_1v = buckets.get("1-v").cloned().unwrap_or_else(Symbol::zero);
    let p_1w = buckets
        .get("1-w")
        .cloned()
        .unwrap_or_else(Symbol::zero);

    let q_uv_extracted = symbol_add(&p_u, &p_v);
    let q_uov_extracted = symbol_sub(&p_u, &p_v);
    let q_w_extracted = p_1w.clone();

    let mut out = Vec::new();
    out.push(format!(
        "bucket sizes: u={}, 1-u={}, v={}, 1-v={}, 1-w={}",
        p_u.terms().count(),
        p_1u.terms().count(),
        p_v.terms().count(),
        p_1v.terms().count(),
        p_1w.terms().count()
    ));
    let p_1u_expected = symbol_scale(&p_u, Coeff::from_integer(-1));
    let p_1v_expected = symbol_scale(&p_v, Coeff::from_integer(-1));
    out.push(diff_symbol_summary("P_1u_vs_-P_u", &p_1u, &p_1u_expected));
    out.push(diff_symbol_summary("P_1v_vs_-P_v", &p_1v, &p_1v_expected));

    let q_blocks = symbol_q_blocks_expanded().expect("q blocks");
    out.push(diff_symbol_summary(
        "q_uv_extracted_vs_golden",
        &q_uv_extracted,
        &q_blocks.q_uv,
    ));
    out.push(diff_symbol_summary(
        "q_uov_extracted_vs_golden",
        &q_uov_extracted,
        &q_blocks.q_u_over_v,
    ));
    out.push(diff_symbol_summary(
        "q_w_extracted_vs_golden",
        &q_w_extracted,
        &q_blocks.q_w,
    ));
    out.join("\n")
}

#[test]
fn psi2_qblock_tail_coeffs() {
    let alpha = pentaladder_alphabet();
    let mut name_map = BTreeMap::new();
    let mut name_to_expr = BTreeMap::new();
    for (expr, name) in alpha.letters.iter().zip(alpha.letter_names.iter()) {
        name_map.insert(expr.to_canonical_string(), name.clone());
        name_to_expr.insert(name.clone(), expr.clone());
    }
    let sym = symbol_psi(2).expect("psi2");
    let buckets = bucket_prefix_by_last(&sym, &name_map);
    let p_u = buckets.get("u").cloned().unwrap_or_else(Symbol::zero);
    let p_v = buckets.get("v").cloned().unwrap_or_else(Symbol::zero);
    let p_1w = buckets
        .get("1-w")
        .cloned()
        .unwrap_or_else(Symbol::zero);
    let q_uv_extracted = symbol_add(&p_u, &p_v);
    let q_w_extracted = p_1w;

    let w = name_to_expr.get("w").expect("w").clone();
    let one_minus_w = name_to_expr.get("1-w").expect("1-w").clone();
    let one_minus_uw = name_to_expr.get("1-uw").expect("1-uw").clone();
    let one_minus_vw = name_to_expr.get("1-vw").expect("1-vw").clone();
    let word_uw = Word(vec![w.clone(), one_minus_w.clone(), one_minus_uw]);
    let word_vw = Word(vec![w, one_minus_w, one_minus_vw]);

    let coeff_uw_q_uv = coeff_for_word(&q_uv_extracted, &word_uw);
    let coeff_vw_q_uv = coeff_for_word(&q_uv_extracted, &word_vw);
    let coeff_uw_q_w = coeff_for_word(&q_w_extracted, &word_uw);
    let coeff_vw_q_w = coeff_for_word(&q_w_extracted, &word_vw);
    assert_eq!(coeff_uw_q_uv, Coeff::from_integer(-1));
    assert_eq!(coeff_vw_q_uv, Coeff::from_integer(-1));
    assert_eq!(coeff_uw_q_w, Coeff::from_integer(-1));
    assert_eq!(coeff_vw_q_w, Coeff::from_integer(-1));
}

fn bucket_prefix_by_last(
    sym: &Symbol,
    name_map: &BTreeMap<String, String>,
) -> BTreeMap<String, Symbol> {
    let mut buckets: BTreeMap<String, BTreeMap<Word, Coeff>> = BTreeMap::new();
    for (word, coeff) in sym.terms() {
        if coeff.is_zero() {
            continue;
        }
        let letters = word.letters();
        if letters.is_empty() {
            continue;
        }
        let last = letters.last().expect("last letter");
        let last_key = last.to_canonical_string();
        let name = name_map
            .get(&last_key)
            .cloned()
            .unwrap_or_else(|| "<unknown>".to_string());
        let prefix = Word(letters[..letters.len() - 1].to_vec());
        let entry = buckets.entry(name).or_default();
        let coeff_entry = entry.entry(prefix).or_insert_with(Coeff::zero);
        *coeff_entry += *coeff;
    }
    let mut out = BTreeMap::new();
    for (name, map) in buckets {
        let terms = map
            .into_iter()
            .filter(|(_, coeff)| !coeff.is_zero())
            .collect::<Vec<_>>();
        out.insert(name, Symbol::from_terms(terms));
    }
    out
}

fn coeff_for_word(sym: &Symbol, word: &Word) -> Coeff {
    sym.terms()
        .find(|(w, _)| *w == word)
        .map(|(_, coeff)| *coeff)
        .unwrap_or_else(Coeff::zero)
}

fn symbol_add(left: &Symbol, right: &Symbol) -> Symbol {
    let mut map = symbol_terms_map(left);
    for (word, coeff) in right.terms() {
        if coeff.is_zero() {
            continue;
        }
        let entry = map.entry(word.clone()).or_insert_with(Coeff::zero);
        *entry += *coeff;
    }
    Symbol::from_terms(map)
}

fn symbol_sub(left: &Symbol, right: &Symbol) -> Symbol {
    let mut map = symbol_terms_map(left);
    for (word, coeff) in right.terms() {
        if coeff.is_zero() {
            continue;
        }
        let entry = map.entry(word.clone()).or_insert_with(Coeff::zero);
        *entry -= *coeff;
    }
    Symbol::from_terms(map)
}

fn symbol_scale(sym: &Symbol, coeff: Coeff) -> Symbol {
    if coeff.is_zero() {
        return Symbol::zero();
    }
    let mut terms = Vec::new();
    for (word, value) in sym.terms() {
        let scaled = *value * coeff;
        if !scaled.is_zero() {
            terms.push((word.clone(), scaled));
        }
    }
    Symbol::from_terms(terms)
}

fn diff_symbol_summary(name: &str, left: &Symbol, right: &Symbol) -> String {
    let left_map = symbol_terms_map(left);
    let right_map = symbol_terms_map(right);
    let mut left_only = Vec::new();
    let mut right_only = Vec::new();
    let mut mismatch = Vec::new();
    for (word, coeff) in &left_map {
        match right_map.get(word) {
            None => left_only.push((word.clone(), *coeff)),
            Some(other) if other != coeff => mismatch.push((word.clone(), *coeff, *other)),
            _ => {}
        }
    }
    for (word, coeff) in &right_map {
        if !left_map.contains_key(word) {
            right_only.push((word.clone(), *coeff));
        }
    }
    left_only.sort_by(|a, b| a.0.cmp(&b.0));
    right_only.sort_by(|a, b| a.0.cmp(&b.0));
    mismatch.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = Vec::new();
    out.push(format!(
        "{name}: left_only={}, right_only={}, coeff_mismatch={}, left_terms={}, right_terms={}",
        left_only.len(),
        right_only.len(),
        mismatch.len(),
        left_map.len(),
        right_map.len()
    ));
    for (word, coeff) in left_only.iter().take(5) {
        out.push(format!("  left_only {word}: {coeff}"));
    }
    for (word, coeff) in right_only.iter().take(5) {
        out.push(format!("  right_only {word}: {coeff}"));
    }
    for (word, left_c, right_c) in mismatch.iter().take(5) {
        out.push(format!("  mismatch {word}: left={left_c}, right={right_c}"));
    }
    out.join("\n")
}
