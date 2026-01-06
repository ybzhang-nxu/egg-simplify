use mpl_ir::parse_sexpr;
use mpl_symbol::space::WordConstraints;

use mpl_rewrite_symbol::{
    fingerprint_expr, Fingerprint, FingerprintBudget, FingerprintCache, FingerprintConfig,
    UnknownReason, WeightFingerprint,
};

fn cfg_with_fuel(fuel: u64) -> FingerprintConfig {
    FingerprintConfig {
        weight_limit: None,
        budget: FingerprintBudget {
            fuel,
            time_limit_ms: None,
        },
        constraints: WordConstraints::default(),
    }
}

#[test]
fn fingerprint_is_stable_over_100_runs_without_cache() {
    let expr = parse_sexpr("(li2 x)").unwrap().normalize();
    let cfg = cfg_with_fuel(100);

    let first = {
        let cache = FingerprintCache::new();
        fingerprint_expr(&expr, &cfg, &cache).unwrap()
    };

    for _ in 0..100 {
        let cache = FingerprintCache::new();
        let next = fingerprint_expr(&expr, &cfg, &cache).unwrap();
        assert_eq!(first, next);
    }
}

#[test]
fn fingerprint_is_stable_over_100_runs_with_cache() {
    let expr = parse_sexpr("(li2 x)").unwrap().normalize();
    let cfg = cfg_with_fuel(100);
    let cache = FingerprintCache::new();
    let first = fingerprint_expr(&expr, &cfg, &cache).unwrap();
    for _ in 0..100 {
        let next = fingerprint_expr(&expr, &cfg, &cache).unwrap();
        assert_eq!(first, next);
    }
}

#[test]
fn unknown_fingerprint_distinguishes_expr_hash() {
    let expr_a = parse_sexpr("(li2 x)").unwrap().normalize();
    let expr_b = parse_sexpr("(li2 y)").unwrap().normalize();
    let cfg = cfg_with_fuel(0);
    let cache = FingerprintCache::new();

    let fp_a = fingerprint_expr(&expr_a, &cfg, &cache).unwrap();
    let fp_b = fingerprint_expr(&expr_b, &cfg, &cache).unwrap();

    match (fp_a, fp_b) {
        (
            Fingerprint::Unknown {
                reason: UnknownReason::BudgetExhausted,
                expr_hash: left,
            },
            Fingerprint::Unknown {
                reason: UnknownReason::BudgetExhausted,
                expr_hash: right,
            },
        ) => {
            assert_ne!(left, right);
        }
        other => panic!("unexpected fingerprints: {other:?}"),
    }
}

#[test]
fn weight_limit_keeps_weight0_information() {
    let expr = parse_sexpr("(+ 7 (li2 x))").unwrap().normalize();
    let mut cfg = cfg_with_fuel(10_000);
    cfg.weight_limit = Some(1);

    let cache = FingerprintCache::new();
    let fp = fingerprint_expr(&expr, &cfg, &cache).unwrap();

    match fp {
        Fingerprint::ByWeight(map) => {
            assert!(map.contains_key(&0), "must keep weight=0");
            match map.get(&2) {
                Some(WeightFingerprint::Unknown { reason, .. }) => {
                    assert_eq!(reason, &UnknownReason::BudgetExhausted);
                }
                other => panic!("unexpected weight=2 entry: {other:?}"),
            }
        }
        other => panic!("unexpected fingerprint: {other:?}"),
    }
}

#[test]
fn golden_expr_hashes_cover_weight_2_3_4() {
    let cache = FingerprintCache::new();

    let expr_w2 = parse_sexpr("(li2 x)").unwrap().normalize();
    let expr_w3 = parse_sexpr("(* (log x) (li2 y))").unwrap().normalize();
    let expr_w4 = parse_sexpr("(* (log x) (log y) (li2 z))")
        .unwrap()
        .normalize();

    let hash_w2 = cache.expr_key(&expr_w2).hash;
    let hash_w3 = cache.expr_key(&expr_w3).hash;
    let hash_w4 = cache.expr_key(&expr_w4).hash;

    assert_eq!(cache.expr_key(&expr_w2).canon.as_ref(), "(li2 x)");
    assert_eq!(
        cache.expr_key(&expr_w3).canon.as_ref(),
        "(* (li2 y) (log x))"
    );
    assert_eq!(
        cache.expr_key(&expr_w4).canon.as_ref(),
        "(* (li2 z) (log x) (log y))"
    );

    assert_eq!(hash_w2, 7_551_655_429_500_050_685);
    assert_eq!(hash_w3, 9_559_116_443_718_576_456);
    assert_eq!(hash_w4, 150_300_114_674_184_897);
}
