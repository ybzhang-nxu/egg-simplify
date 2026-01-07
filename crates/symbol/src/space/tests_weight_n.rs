#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::{Mutex, OnceLock};

    use mpl_ir::{parse_sexpr, Expr};
    use num_rational::Rational64;
    use num_traits::Zero;

    use crate::error::SymbolError;
    use crate::{Coeff, Symbol, Word};

    use crate::space::{
        build_integrable_basis, check_integrable_n, reduce_to_basis, Alphabet, WordConstraints,
    };

    static PRINT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn r(n: i64, d: i64) -> Coeff {
        Rational64::new(n, d)
    }

    fn var(name: &str) -> Expr {
        Expr::Var(name.to_string()).normalize()
    }

    fn word_from_ids(alpha: &Alphabet, ids: &[usize]) -> Word {
        Word(ids.iter().map(|&i| alpha.letters[i].clone()).collect())
    }

    fn symbol_single_word(alpha: &Alphabet, ids: &[usize], c: Coeff) -> Symbol {
        let mut s = Symbol::zero();
        s.add_term(word_from_ids(alpha, ids), c);
        s
    }

    fn symbol_from_vector(alpha: &Alphabet, words: &[Vec<usize>], vec: &[Coeff]) -> Symbol {
        assert_eq!(words.len(), vec.len());
        let mut s = Symbol::zero();
        for (w_ids, c) in words.iter().zip(vec.iter()) {
            if c.is_zero() {
                continue;
            }
            s.add_term(word_from_ids(alpha, w_ids), *c);
        }
        s
    }

    fn combine_basis(
        alpha: &Alphabet,
        words: &[Vec<usize>],
        basis_vecs: &[Vec<Coeff>],
        coeffs: &[Coeff],
    ) -> Symbol {
        assert_eq!(basis_vecs.len(), coeffs.len());
        let ncols = words.len();
        let mut out = Symbol::zero();
        for (i, ci) in coeffs.iter().enumerate() {
            if ci.is_zero() {
                continue;
            }
            let v = &basis_vecs[i];
            assert_eq!(v.len(), ncols);
            for col in 0..ncols {
                let add = *ci * v[col];
                if add.is_zero() {
                    continue;
                }
                out.add_term(word_from_ids(alpha, &words[col]), add);
            }
        }
        out
    }

    fn permutations(items: &[Expr]) -> Vec<Vec<Expr>> {
        fn rec(
            items: &[Expr],
            used: &mut Vec<bool>,
            cur: &mut Vec<Expr>,
            out: &mut Vec<Vec<Expr>>,
        ) {
            if cur.len() == items.len() {
                out.push(cur.clone());
                return;
            }
            for i in 0..items.len() {
                if used[i] {
                    continue;
                }
                used[i] = true;
                cur.push(items[i].clone());
                rec(items, used, cur, out);
                cur.pop();
                used[i] = false;
            }
        }
        let mut used = vec![false; items.len()];
        let mut cur = Vec::with_capacity(items.len());
        let mut out = Vec::new();
        rec(items, &mut used, &mut cur, &mut out);
        out
    }

    fn symbol_from_words(words: Vec<Vec<Expr>>) -> Symbol {
        let mut s = Symbol::zero();
        for w in words {
            s.add_term(Word(w), r(1, 1));
        }
        s
    }

    fn print_stats(stats: &crate::space::BasisStats) {
        let _guard = PRINT_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        eprintln!("\n{}", stats.one_line());
    }

    fn multiset_words_xy(a: usize, b: usize, idx_x: usize, idx_y: usize) -> Vec<Vec<usize>> {
        fn rec(
            a: usize,
            b: usize,
            idx_x: usize,
            idx_y: usize,
            cur: &mut Vec<usize>,
            out: &mut Vec<Vec<usize>>,
        ) {
            if a == 0 && b == 0 {
                out.push(cur.clone());
                return;
            }
            if a > 0 {
                cur.push(idx_x);
                rec(a - 1, b, idx_x, idx_y, cur, out);
                cur.pop();
            }
            if b > 0 {
                cur.push(idx_y);
                rec(a, b - 1, idx_x, idx_y, cur, out);
                cur.pop();
            }
        }
        let mut out = Vec::new();
        let mut cur = Vec::new();
        rec(a, b, idx_x, idx_y, &mut cur, &mut out);
        out
    }

    fn toy_alphabet_xyz() -> Alphabet {
        Alphabet {
            name: "toy_xyz".to_string(),
            letters: vec![var("x"), var("y"), var("z")],
            letter_names: vec!["x".into(), "y".into(), "z".into()],
        }
    }

    fn toy_alphabet_xy() -> Alphabet {
        Alphabet {
            name: "toy_xy".to_string(),
            letters: vec![var("x"), var("y")],
            letter_names: vec!["x".into(), "y".into()],
        }
    }

    fn no_constraints() -> WordConstraints {
        WordConstraints {
            first_allowed: None,
            allowed_pairs: None,
        }
    }

    #[test]
    fn integrability_trivial_weight0_weight1() {
        let alpha = toy_alphabet_xy();

        let s0 = Symbol::zero();
        assert!(check_integrable_n(&s0).unwrap());

        let s1 = symbol_single_word(&alpha, &[0], r(3, 1));
        assert!(check_integrable_n(&s1).unwrap());
    }

    #[test]
    fn integrability_weight3_shuffle_is_integrable_single_word_is_not() {
        let x = var("x");
        let y = var("y");
        let z = var("z");

        let shuf_words = permutations(&[x.clone(), y.clone(), z.clone()]);
        let shuf = symbol_from_words(shuf_words);
        assert!(check_integrable_n(&shuf).unwrap());

        let mut bad = Symbol::zero();
        bad.add_term(Word(vec![x, y, z]), r(1, 1));
        assert!(!check_integrable_n(&bad).unwrap());
    }

    #[test]
    fn integrability_weight4_shuffle_is_integrable_single_word_is_not() {
        let a = var("a");
        let b = var("b");
        let c = var("c");
        let d = var("d");

        let shuf_words = permutations(&[a.clone(), b.clone(), c.clone(), d.clone()]);
        let shuf = symbol_from_words(shuf_words);
        assert!(check_integrable_n(&shuf).unwrap());

        let mut bad = Symbol::zero();
        bad.add_term(Word(vec![a, b, c, d]), r(1, 1));
        assert!(!check_integrable_n(&bad).unwrap());
    }

    #[test]
    fn basis_dim_xy_matches_w_plus_1_for_w2_to_w6() {
        let alpha = toy_alphabet_xy();
        let c = no_constraints();

        for w in 2..=6 {
            let basis = build_integrable_basis(&alpha, &c, w).unwrap();

            let expected_words = 1usize << w;
            assert_eq!(basis.words.len(), expected_words);

            assert_eq!(basis.vectors.len(), w + 1);

            let basis2 = build_integrable_basis(&alpha, &c, w).unwrap();
            assert_eq!(basis.words, basis2.words);
            assert_eq!(basis.vectors, basis2.vectors);

            for v in &basis.vectors {
                let s = symbol_from_vector(&alpha, &basis.words, v);
                assert!(check_integrable_n(&s).unwrap());
            }
        }
    }

    #[test]
    fn basis_stats_are_deterministic() {
        let alpha = toy_alphabet_xy();
        let c = no_constraints();
        let w = 5;

        let b1 = build_integrable_basis(&alpha, &c, w).unwrap();
        let b2 = build_integrable_basis(&alpha, &c, w).unwrap();

        assert_eq!(b1.stats(), b2.stats());
        assert_eq!(b1.stats().one_line(), b2.stats().one_line());
    }

    #[test]
    fn basis_word_order_xy_is_lexicographic() {
        let alpha = toy_alphabet_xy();
        let c = no_constraints();
        let w = 4;
        let basis = build_integrable_basis(&alpha, &c, w).unwrap();

        assert_eq!(basis.words[0], vec![0, 0, 0, 0]);
        assert_eq!(basis.words[1], vec![0, 0, 0, 1]);
        assert_eq!(basis.words[2], vec![0, 0, 1, 0]);
        assert_eq!(basis.words[3], vec![0, 0, 1, 1]);
        assert_eq!(basis.words.last().unwrap(), &vec![1, 1, 1, 1]);
    }

    #[test]
    fn reduce_to_basis_recovers_coeffs_weight6_xy() {
        let alpha = toy_alphabet_xy();
        let c = no_constraints();
        let w = 6;

        let basis = build_integrable_basis(&alpha, &c, w).unwrap();
        assert_eq!(basis.vectors.len(), 7);

        let coeffs_in: Vec<Coeff> = vec![
            r(1, 1),
            r(-2, 3),
            r(5, 7),
            r(0, 1),
            r(11, 13),
            r(-1, 2),
            r(3, 5),
        ];

        let sym = combine_basis(&alpha, &basis.words, &basis.vectors, &coeffs_in);
        assert!(check_integrable_n(&sym).unwrap());

        let (coeffs_out, residual) = reduce_to_basis(&sym, &basis, &alpha).unwrap();
        assert!(
            residual.is_zero(),
            "expected zero residual for in-space symbol"
        );
        assert_eq!(coeffs_out, coeffs_in);

        let (coeffs_out2, residual2) = reduce_to_basis(&sym, &basis, &alpha).unwrap();
        assert_eq!(coeffs_out2, coeffs_out);
        assert_eq!(residual2, residual);
    }

    #[test]
    fn reduce_to_basis_nonintegrable_has_nonzero_residual_or_error() {
        let alpha = toy_alphabet_xy();
        let c = no_constraints();
        let w = 4;
        let basis = build_integrable_basis(&alpha, &c, w).unwrap();

        let sym = symbol_single_word(&alpha, &[0, 1, 0, 1], r(1, 1));
        assert!(!check_integrable_n(&sym).unwrap());

        match reduce_to_basis(&sym, &basis, &alpha) {
            Ok((_coeffs, residual)) => {
                assert!(
                    !residual.is_zero(),
                    "expected nonzero residual for out-of-space symbol"
                );
            }
            Err(_e) => {}
        }
    }

    #[test]
    fn integrability_errors_on_degenerate_letter_all_samples_invalid() {
        let zero = parse_sexpr("(+ x (* -1 x))").unwrap().normalize();
        let x = var("x");
        let y = var("y");

        let mut sym = Symbol::zero();
        sym.add_term(Word(vec![zero, x, y]), r(1, 1));

        let err = check_integrable_n(&sym).unwrap_err();
        assert!(matches!(err, SymbolError::InsufficientSamples));
    }

    #[ignore]
    #[test]
    fn stress_basis_xy_weight10_dim11() {
        let alpha = toy_alphabet_xy();
        let c = no_constraints();
        let w = 10;

        let basis = build_integrable_basis(&alpha, &c, w).unwrap();
        print_stats(basis.stats());
        assert_eq!(basis.stats().dim, w + 1);
        assert_eq!(basis.words.len(), 1usize << w);
        assert_eq!(basis.vectors.len(), w + 1);

        let s0 = symbol_from_vector(&alpha, &basis.words, &basis.vectors[0]);
        assert!(check_integrable_n(&s0).unwrap());
    }

    #[ignore]
    #[test]
    fn stress_basis_xy_weight12_dim13() {
        let alpha = toy_alphabet_xy();
        let c = no_constraints();
        let w = 12;

        let basis = build_integrable_basis(&alpha, &c, w).unwrap();
        print_stats(basis.stats());
        assert_eq!(basis.words.len(), 1usize << w);
        assert_eq!(basis.vectors.len(), w + 1);

        let idx_x = 0usize;
        let idx_y = 1usize;
        let words = multiset_words_xy(5, 7, idx_x, idx_y);

        let mut sym = Symbol::zero();
        for w_ids in words {
            sym.add_term(word_from_ids(&alpha, &w_ids), r(1, 1));
        }
        assert!(check_integrable_n(&sym).unwrap());

        let (_coeffs, residual) = reduce_to_basis(&sym, &basis, &alpha).unwrap();
        assert!(residual.is_zero());
    }

    #[ignore]
    #[test]
    fn stress_build_basis_xyz_weight8_smoke() {
        let alpha = toy_alphabet_xyz();
        let c = no_constraints();
        let w = 8;

        let b1 = build_integrable_basis(&alpha, &c, w).unwrap();
        let b2 = build_integrable_basis(&alpha, &c, w).unwrap();
        print_stats(b1.stats());
        assert_eq!(b1.words, b2.words);
        assert_eq!(b1.vectors, b2.vectors);

        for i in 0..b1.vectors.len().min(3) {
            let s = symbol_from_vector(&alpha, &b1.words, &b1.vectors[i]);
            assert!(check_integrable_n(&s).unwrap());
        }
    }

    #[ignore]
    #[test]
    fn stress_basis_xy_weight14_dim15() {
        let alpha = toy_alphabet_xy();
        let c = no_constraints();
        let w = 14;

        let basis = build_integrable_basis(&alpha, &c, w).unwrap();
        print_stats(basis.stats());
        assert_eq!(basis.words.len(), 1usize << w);
        assert_eq!(basis.vectors.len(), w + 1);

        let s0 = symbol_from_vector(&alpha, &basis.words, &basis.vectors[0]);
        assert!(check_integrable_n(&s0).unwrap());
    }

    #[ignore]
    #[test]
    fn stress_basis_xy_weight16_dim17() {
        let alpha = toy_alphabet_xy();
        let c = no_constraints();
        let w = 16;

        let basis = build_integrable_basis(&alpha, &c, w).unwrap();
        print_stats(basis.stats());
        assert_eq!(basis.words.len(), 1usize << w);
        assert_eq!(basis.vectors.len(), w + 1);
    }

    #[ignore]
    #[test]
    fn stress_build_basis_xyz_weight9_smoke() {
        let alpha = toy_alphabet_xyz();
        let c = no_constraints();
        let w = 9;

        let b1 = build_integrable_basis(&alpha, &c, w).unwrap();
        let b2 = build_integrable_basis(&alpha, &c, w).unwrap();
        print_stats(b1.stats());
        assert_eq!(b1.words, b2.words);
        assert_eq!(b1.vectors, b2.vectors);

        for i in 0..b1.vectors.len().min(2) {
            let s = symbol_from_vector(&alpha, &b1.words, &b1.vectors[i]);
            assert!(check_integrable_n(&s).unwrap());
        }
    }

    #[ignore]
    #[test]
    fn stress_build_basis_xyz_weight10_smoke() {
        let alpha = toy_alphabet_xyz();
        let c = no_constraints();
        let w = 10;

        let b1 = build_integrable_basis(&alpha, &c, w).unwrap();
        let b2 = build_integrable_basis(&alpha, &c, w).unwrap();
        print_stats(b1.stats());
        assert_eq!(b1.words, b2.words);
        assert_eq!(b1.vectors, b2.vectors);
    }

    #[ignore]
    #[test]
    fn stress_check_integrable_weight40_alternating() {
        let alpha = toy_alphabet_xy();
        let mut ids = Vec::with_capacity(40);
        for i in 0..40 {
            ids.push(i % 2);
        }
        let sym = symbol_single_word(&alpha, &ids, r(1, 1));
        assert!(!check_integrable_n(&sym).unwrap());
    }

    #[test]
    fn constraints_affect_word_count_deterministically() {
        let alpha = toy_alphabet_xyz();

        let mut first = BTreeSet::new();
        first.insert(0usize);
        first.insert(1usize);

        let c = WordConstraints {
            first_allowed: Some(first),
            allowed_pairs: None,
        };

        let b = build_integrable_basis(&alpha, &c, 4).unwrap();
        assert_eq!(b.words.len(), 54);
        let b2 = build_integrable_basis(&alpha, &c, 4).unwrap();
        assert_eq!(b.words, b2.words);
        assert_eq!(b.vectors, b2.vectors);
    }
}
