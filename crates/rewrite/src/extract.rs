use egg::{Extractor, Id, RecExpr, Runner};

use mpl_ir::Expr;

use crate::config::RewriteError;
use crate::lang::{ConstFold, Lang};

pub(crate) fn extract_best(runner: &Runner<Lang, ConstFold>) -> RecExpr<Lang> {
    let root = runner.roots[0];
    let extractor = Extractor::new(&runner.egraph, egg::AstSize);
    let (_cost, best) = extractor.find_best(root);
    best
}

pub fn lift_expr(expr: &RecExpr<Lang>) -> Result<Expr, RewriteError> {
    let root = Id::from(expr.as_ref().len() - 1);
    let lifted = lift(expr, root)?;
    Ok(lifted.normalize())
}

fn lift(rec: &RecExpr<Lang>, id: Id) -> Result<Expr, RewriteError> {
    match &rec[id] {
        Lang::Num(n) => Ok(Expr::Rational(*n)),
        Lang::Var(sym) => Ok(Expr::Var(sym.to_string())),
        Lang::Add([a, b]) => Ok(Expr::Add(vec![lift(rec, *a)?, lift(rec, *b)?])),
        Lang::Mul([a, b]) => Ok(Expr::Mul(vec![lift(rec, *a)?, lift(rec, *b)?])),
        Lang::Log(inner) => Ok(Expr::Log(Box::new(lift(rec, *inner)?))),
        Lang::Li2(inner) => Ok(Expr::Li2(Box::new(lift(rec, *inner)?))),
        Lang::Pow([a, b]) => {
            let exp = match &rec[*b] {
                Lang::Num(value) if value.is_integer() => {
                    let numer = *value.numer();
                    i32::try_from(numer).map_err(|_| {
                        RewriteError::InvalidExponent(format!("exponent out of range: {}", value))
                    })?
                }
                Lang::Num(value) => {
                    return Err(RewriteError::InvalidExponent(format!(
                        "non-integer exponent: {}",
                        value
                    )));
                }
                _ => {
                    return Err(RewriteError::InvalidExponent(
                        "exponent must be numeric".to_string(),
                    ));
                }
            };
            Ok(Expr::Pow(Box::new(lift(rec, *a)?), exp))
        }
    }
}
