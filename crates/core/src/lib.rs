use egg::{rewrite as rw, *};
use thiserror::Error;

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

    Ok(best.to_string())
}
