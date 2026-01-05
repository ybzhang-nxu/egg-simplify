use egg::{define_language, Analysis, DidMerge, EGraph, Id, Symbol};
use num_rational::Rational64;
use num_traits::Zero;

define_language! {
    pub enum Lang {
        Num(Rational64),
        Var(Symbol),
        "+" = Add([Id; 2]),
        "*" = Mul([Id; 2]),
        "^" = Pow([Id; 2]),
        "log" = Log(Id),
        "li2" = Li2(Id),
    }
}

#[derive(Default)]
pub(crate) struct ConstFold;
type EGraphCF = EGraph<Lang, ConstFold>;

impl Analysis<Lang> for ConstFold {
    type Data = Option<Rational64>;

    fn make(egraph: &EGraphCF, enode: &Lang) -> Self::Data {
        match enode {
            Lang::Num(n) => Some(*n),
            Lang::Add([a, b]) => Some(egraph[*a].data? + egraph[*b].data?),
            Lang::Mul([a, b]) => Some(egraph[*a].data? * egraph[*b].data?),
            Lang::Pow([a, b]) => {
                let base = egraph[*a].data?;
                let exp_r = egraph[*b].data?;
                if !exp_r.is_integer() {
                    return None;
                }
                let exp_num = *exp_r.numer();
                let exp = i32::try_from(exp_num).ok()?;
                if exp < 0 && base.is_zero() {
                    return None;
                }
                Some(base.pow(exp))
            }
            Lang::Log(_) | Lang::Li2(_) | Lang::Var(_) => None,
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
            let const_id = egraph.add(Lang::Num(c));
            egraph.union(id, const_id);
        }
    }
}
