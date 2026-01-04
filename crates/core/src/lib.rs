use egg::{rewrite as rw, *};
use thiserror::Error;
use std::cmp::Ordering;

define_language! {
    pub enum Sym {
        Num(i64),
        Symbol(Symbol),

        "+" = Add([Id; 2]),
        "*" = Mul([Id; 2]),
        "^" = Pow([Id; 2]),
        "neg" = Neg(Id),
    }
}

#[derive(Debug, Error)]
pub enum SimplifyError {
    #[error("parse error: {0}")]
    Parse(String),
}

fn rules() -> Vec<Rewrite<Sym, ConstFold>> {
    vec![
        rw!("add-comm"; "(+ ?a ?b)" => "(+ ?b ?a)"),
        rw!("mul-comm"; "(* ?a ?b)" => "(* ?b ?a)"),

        rw!("add-assoc"; "(+ ?a (+ ?b ?c))" => "(+ (+ ?a ?b) ?c)"),
        rw!("mul-assoc"; "(* ?a (* ?b ?c))" => "(* (* ?a ?b) ?c)"),

        rw!("add-zero"; "(+ ?a 0)" => "?a"),
        rw!("add-zero2"; "(+ 0 ?a)" => "?a"),
        rw!("mul-one"; "(* ?a 1)" => "?a"),
        rw!("mul-one2"; "(* 1 ?a)" => "?a"),
        rw!("mul-zero"; "(* ?a 0)" => "0"),
        rw!("mul-zero2"; "(* 0 ?a)" => "0"),

        rw!("pow-1"; "(^ ?a 1)" => "?a"),
        rw!("pow-0"; "(^ ?a 0)" => "1"),

        rw!("neg-neg"; "(neg (neg ?a))" => "?a"),
        rw!("neg-add"; "(neg (+ ?a ?b))" => "(+ (neg ?a) (neg ?b))"),
    ]
}


#[derive(Default)]
struct ConstFold;
type EGraphCF = EGraph<Sym, ConstFold>;

impl Analysis<Sym> for ConstFold {
    type Data = Option<i64>;

    fn make(egraph: &EGraphCF, enode: &Sym) -> Self::Data {
        match enode {
            Sym::Num(n) => Some(*n),

            Sym::Add([a, b]) => Some(egraph[*a].data? + egraph[*b].data?),
            Sym::Mul([a, b]) => Some(egraph[*a].data? * egraph[*b].data?),
            Sym::Neg(a) => Some(-egraph[*a].data?),

            // pow: (^ base exp) where exp is nonnegative small integer
            Sym::Pow([a, b]) => {
                let base = egraph[*a].data?;
                let exp = egraph[*b].data?;
                if exp < 0 {
                    return None;
                }
                let mut v = 1i64;
                for _ in 0..exp {
                    v = v.saturating_mul(base);
                }
                Some(v)
            }

            _ => None,
        }
    }


    fn merge(&mut self, to: &mut Self::Data, from: Self::Data) -> DidMerge {
        let before = *to;
        if to.is_none() && from.is_some() {
            *to = from;
        }
        DidMerge(before != *to, false)
    }

    fn modify(egraph: &mut EGraphCF, id: Id) {
        if let Some(c) = egraph[id].data {
            let const_id = egraph.add(Sym::Num(c));
            egraph.union(id, const_id);
        }
    }
}

pub fn simplify_sexp(input: &str, iters: usize) -> Result<String, SimplifyError> {
    let expr: RecExpr<Sym> = input
        .parse()
        .map_err(|e| SimplifyError::Parse(format!("{e:?}")))?;

    let runner = Runner::<Sym, ConstFold>::default()
    .with_expr(&expr)
    .with_iter_limit(iters)
    .with_node_limit(50_000)
    .with_time_limit(std::time::Duration::from_secs(2))
    .run(&rules());

    let root = runner.roots[0];
    let extractor = Extractor::new(&runner.egraph, AstSize);
    let (_best_cost, best) = extractor.find_best(root);

    Ok(normalize_expr(&best))
}

fn normalize_expr(expr: &RecExpr<Sym>) -> String {
    let root = Id::from(expr.as_ref().len() - 1);
    format_node(expr, root)
}

fn format_node(expr: &RecExpr<Sym>, id: Id) -> String {
    match &expr[id] {
        Sym::Num(n) => n.to_string(),
        Sym::Symbol(sym) => sym.to_string(),
        Sym::Add(_) => format_flat(expr, id, '+'),
        Sym::Mul(_) => format_flat(expr, id, '*'),
        Sym::Neg(child) => format!("(neg {})", format_node(expr, *child)),
        Sym::Pow([a, b]) => format!("(^ {} {})", format_node(expr, *a), format_node(expr, *b)),
    }
}

fn format_flat(expr: &RecExpr<Sym>, id: Id, op: char) -> String {
    let mut children = Vec::new();
    collect_flat(expr, id, op, &mut children);

    let mut parts: Vec<String> = children
        .into_iter()
        .map(|child| format_node(expr, child))
        .collect();
    parts.sort_by(sort_terms);

    let joined = parts.join(" ");
    format!("({} {})", op, joined)
}

fn collect_flat(expr: &RecExpr<Sym>, id: Id, op: char, out: &mut Vec<Id>) {
    match (&expr[id], op) {
        (Sym::Add([a, b]), '+') => {
            collect_flat(expr, *a, op, out);
            collect_flat(expr, *b, op, out);
        }
        (Sym::Mul([a, b]), '*') => {
            collect_flat(expr, *a, op, out);
            collect_flat(expr, *b, op, out);
        }
        _ => out.push(id),
    }
}

fn sort_terms(a: &String, b: &String) -> Ordering {
    let a_num = is_number(a);
    let b_num = is_number(b);
    match (a_num, b_num) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a.cmp(b),
    }
}

fn is_number(s: &str) -> bool {
    let trimmed = s.trim_start_matches('-');
    !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit())
}
