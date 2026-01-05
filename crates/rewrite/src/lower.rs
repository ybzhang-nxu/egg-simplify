use egg::{Id, RecExpr};
use num_rational::Rational64;

use mpl_ir::Expr;

use crate::config::RewriteError;
use crate::lang::Lang;

pub fn lower_expr(expr: &Expr) -> Result<RecExpr<Lang>, RewriteError> {
    let mut builder = RecExpr::default();
    lower_into(expr, &mut builder)?;
    Ok(builder)
}

fn lower_into(expr: &Expr, builder: &mut RecExpr<Lang>) -> Result<Id, RewriteError> {
    match expr {
        Expr::Rational(value) => Ok(builder.add(Lang::Num(*value))),
        Expr::Var(name) => Ok(builder.add(Lang::Var(egg::Symbol::from(name.as_str())))),
        Expr::Add(items) => lower_nary(items, builder, |a, b| Lang::Add([a, b])),
        Expr::Mul(items) => lower_nary(items, builder, |a, b| Lang::Mul([a, b])),
        Expr::Pow(base, exp) => {
            let base_id = lower_into(base, builder)?;
            let exp_id = builder.add(Lang::Num(Rational64::from_integer(*exp as i64)));
            Ok(builder.add(Lang::Pow([base_id, exp_id])))
        }
        Expr::Log(inner) => {
            let child = lower_into(inner, builder)?;
            Ok(builder.add(Lang::Log(child)))
        }
        Expr::Li2(inner) => {
            let child = lower_into(inner, builder)?;
            Ok(builder.add(Lang::Li2(child)))
        }
        Expr::Neg(inner) => {
            let minus_one = builder.add(Lang::Num(Rational64::from_integer(-1)));
            let child = lower_into(inner, builder)?;
            Ok(builder.add(Lang::Mul([minus_one, child])))
        }
    }
}

fn lower_nary<F>(items: &[Expr], builder: &mut RecExpr<Lang>, make: F) -> Result<Id, RewriteError>
where
    F: Fn(Id, Id) -> Lang,
{
    if items.is_empty() {
        return Err(RewriteError::InvalidArity(
            "n-ary operator requires at least one operand".to_string(),
        ));
    }
    if items.len() == 1 {
        return lower_into(&items[0], builder);
    }
    // Right associative fold for deterministic structure.
    let mut iter = items.iter().rev();
    let mut acc = lower_into(iter.next().unwrap(), builder)?;
    for item in iter {
        let left = lower_into(item, builder)?;
        acc = builder.add(make(left, acc));
    }
    Ok(acc)
}
