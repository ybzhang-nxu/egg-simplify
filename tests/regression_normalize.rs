use mpl_ir::parse_sexpr;

struct Case {
    name: &'static str,
    input: &'static str,
    expected: &'static str,
}

fn canon(input: &str) -> String {
    parse_sexpr(input)
        .expect("parse")
        .normalize()
        .to_canonical_string()
}

fn cases() -> Vec<Case> {
    vec![
        // Group I - Parsing & Flattening.
        Case {
            name: "I1_flatten_add",
            input: "(+ x (+ y z))",
            expected: "(+ x y z)",
        },
        Case {
            name: "I2_flatten_mul",
            input: "(* x (* y z))",
            expected: "(* x y z)",
        },
        Case {
            name: "I3_flatten_nested_add",
            input: "(+ x (+ (+ y 0) (+ 3 x)))",
            expected: "(+ 3 x x y)",
        },
        Case {
            name: "I4_flatten_mul_constants",
            input: "(* y x 2 1 z)",
            expected: "(* 2 x y z)",
        },
        Case {
            name: "I5_singleton_add",
            input: "(+ (+ (+ x)))",
            expected: "x",
        },
        // Group II - Identity & Annihilator.
        Case {
            name: "II6_add_zero",
            input: "(+ x 0)",
            expected: "x",
        },
        Case {
            name: "II7_mul_one",
            input: "(* x 1)",
            expected: "x",
        },
        Case {
            name: "II8_mul_zero",
            input: "(* x 0 y)",
            expected: "0",
        },
        Case {
            name: "II9_mul_single_zero",
            input: "(* 0)",
            expected: "0",
        },
        Case {
            name: "II10_empty_add",
            input: "(+ )",
            expected: "0",
        },
        // Group III - Rational Folding.
        Case {
            name: "III11_add_rationals",
            input: "(+ 1/2 1/3)",
            expected: "5/6",
        },
        Case {
            name: "III12_add_rationals_with_var",
            input: "(+ 1/2 1/3 x)",
            expected: "(+ 5/6 x)",
        },
        Case {
            name: "III13_mul_rationals",
            input: "(* 2 3)",
            expected: "6",
        },
        Case {
            name: "III14_mul_rationals_with_var",
            input: "(* 2 3 x)",
            expected: "(* 6 x)",
        },
        Case {
            name: "III15_mul_negative_constants",
            input: "(* -2 -3 x)",
            expected: "(* 6 x)",
        },
        // Group IV - Division Normalization.
        Case {
            name: "IV16_div_basic",
            input: "(/ 1 2)",
            expected: "1/2",
        },
        Case {
            name: "IV17_div_mul_simplify",
            input: "(* 2 (/ 1 2))",
            expected: "1",
        },
        Case {
            name: "IV18_div_var",
            input: "(/ x y)",
            expected: "(* (^ y -1) x)",
        },
        Case {
            name: "IV19_div_nary",
            input: "(/ x y z)",
            expected: "(* (^ y -1) (^ z -1) x)",
        },
        Case {
            name: "IV20_div_with_mul",
            input: "(/ (* x y) z)",
            expected: "(* (^ z -1) x y)",
        },
        // Group V - Powers & Exponents.
        Case {
            name: "V21_pow_zero",
            input: "(^ x 0)",
            expected: "1",
        },
        Case {
            name: "V22_pow_nesting",
            input: "(^ (^ x 2) 3)",
            expected: "(^ x 6)",
        },
        Case {
            name: "V23_pow_merge",
            input: "(* (^ x 2) (^ x 3))",
            expected: "(^ x 5)",
        },
        Case {
            name: "V24_pow_merge_with_var",
            input: "(* x (^ x 3))",
            expected: "(^ x 4)",
        },
        Case {
            name: "V25_pow_rational_negative_exp",
            input: "(^ 1/2 -2)",
            expected: "4",
        },
        // Group VI - Signs & Stability.
        Case {
            name: "VI26_double_neg",
            input: "(- (- x))",
            expected: "x",
        },
        Case {
            name: "VI27_neg_const_in_mul",
            input: "(* (- 2) x)",
            expected: "(* -2 x)",
        },
        Case {
            name: "VI28_double_neg_in_mul",
            input: "(* (- x) (- y))",
            expected: "(* x y)",
        },
    ]
}

#[test]
fn regression_cases() {
    for case in cases() {
        let actual = canon(case.input);
        assert_eq!(
            actual, case.expected,
            "case {} input {}",
            case.name, case.input
        );
    }
}

#[test]
fn regression_roundtrip_idempotent() {
    let input = "(+ x (+ y 0) (+ 3 x))";
    let first = canon(input);
    let second = canon(&first);
    assert_eq!(first, second);
}

#[test]
fn regression_determinism() {
    let input = "(* 2 (/ 1 2) (^ x 3) (^ x -1))";
    let first = canon(input);
    for _ in 0..10 {
        assert_eq!(first, canon(input));
    }
}
