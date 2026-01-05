use egg::{rewrite as rw, Rewrite};

use crate::lang::{ConstFold, Lang};

pub(crate) fn safe_rules() -> Vec<Rewrite<Lang, ConstFold>> {
    vec![
        rw!("add-zero"; "(+ ?a 0)" => "?a"),
        rw!("add-zero-comm"; "(+ 0 ?a)" => "?a"),
        rw!("mul-one"; "(* ?a 1)" => "?a"),
        rw!("mul-one-comm"; "(* 1 ?a)" => "?a"),
        rw!("mul-zero"; "(* ?a 0)" => "0"),
        rw!("mul-zero-comm"; "(* 0 ?a)" => "0"),
        rw!("pow-one"; "(^ ?a 1)" => "?a"),
        rw!("pow-zero"; "(^ ?a 0)" => "1"),
    ]
}

pub(crate) fn aggressive_rules() -> Vec<Rewrite<Lang, ConstFold>> {
    vec![
        rw!("factor-left"; "(+ (* ?a ?b) (* ?a ?c))" => "(* ?a (+ ?b ?c))"),
        rw!("factor-right"; "(+ (* ?b ?a) (* ?c ?a))" => "(* (+ ?b ?c) ?a)"),
    ]
}
