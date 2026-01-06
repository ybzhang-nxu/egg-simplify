use egg::{rewrite as rw, Analysis, Rewrite};

use crate::lang::Lang;

pub(crate) fn safe_rules<N>() -> Vec<Rewrite<Lang, N>>
where
    N: Analysis<Lang> + 'static,
{
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

pub(crate) fn aggressive_rules<N>() -> Vec<Rewrite<Lang, N>>
where
    N: Analysis<Lang> + 'static,
{
    vec![
        rw!("factor-left"; "(+ (* ?a ?b) (* ?a ?c))" => "(* ?a (+ ?b ?c))"),
        rw!("factor-right"; "(+ (* ?b ?a) (* ?c ?a))" => "(* (+ ?b ?c) ?a)"),
    ]
}
