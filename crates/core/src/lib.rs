use egg::{rewrite as rw, *};
use itertools::Itertools;
use num_rational::Rational64;
use num_traits::{One, Zero};
use std::collections::BTreeMap;
use std::time::Duration;
use symbolic_expressions::Sexp;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SimplifyError {
    #[error("parse error: {0}")]
    Parse(String),
}

/// Front-end IR with n-ary Add/Mul and rational constants.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Expr {
    Const(Rational64),
    Symbol(String),
    Add(Vec<Expr>),
    Mul(Vec<Expr>),
    Pow(Box<Expr>, i32),
    Neg(Box<Expr>),
}

impl Expr {
    fn is_const_zero(&self) -> bool {
        matches!(self, Expr::Const(c) if c.is_zero())
    }
}

/// Entry point used by CLI.
pub fn simplify_sexp(input: &str, iters: usize) -> Result<String, SimplifyError> {
    let parsed = parse_sexpr(input)?;
    let normalized = normalize(parsed);
    let lowered = lower_to_egg(&normalized)?;
    let runner = build_runner(lowered.clone(), iters);
    let best = extract_best(&runner);
    let lifted = lift_from_egg(&best);
    let out = normalize(lifted);
    Ok(to_canonical_string(&out))
}

fn build_runner(expr: RecExpr<Lang>, iters: usize) -> Runner<Lang, ConstFold> {
    Runner::<Lang, ConstFold>::default()
        .with_expr(&expr)
        .with_iter_limit(iters)
        .with_node_limit(50_000)
        .with_time_limit(Duration::from_secs(2))
        .run(&rules())
}

fn extract_best(runner: &Runner<Lang, ConstFold>) -> RecExpr<Lang> {
    let root = runner.roots[0];
    let extractor = Extractor::new(&runner.egraph, AstSize);
    let (_cost, best) = extractor.find_best(root);
    best
}

// ======================
// Parsing
// ======================

pub fn parse_sexpr(input: &str) -> Result<Expr, SimplifyError> {
    let sexp = symbolic_expressions::parser::parse_str(input)
        .map_err(|e| SimplifyError::Parse(format!("{e}")))?;
    sexp_to_expr(&sexp)
}

fn sexp_to_expr(sexp: &Sexp) -> Result<Expr, SimplifyError> {
    match sexp {
        Sexp::String(atom) => parse_atom(atom)
            .ok_or_else(|| SimplifyError::Parse(format!("unsupported atom `{atom}`"))),
        Sexp::List(list) => {
            if list.is_empty() {
                return Err(SimplifyError::Parse("empty list".into()));
            }
            let head = &list[0];
            let head_atom = match head {
                Sexp::String(a) => a.clone(),
                _ => return Err(SimplifyError::Parse("operator must be atom".into())),
            };
            let tail = &list[1..];
            match head_atom.as_str() {
                "+" => parse_nary("+", tail, Expr::Add),
                "*" => parse_nary("*", tail, Expr::Mul),
                "/" => parse_div(tail),
                "^" => parse_pow(tail),
                "neg" => parse_neg(tail),
                "-" => {
                    if tail.len() == 1 {
                        parse_neg(tail)
                    } else {
                        Err(SimplifyError::Parse(
                            "only unary (- x) is supported; use (+ a (neg b)) for subtraction"
                                .into(),
                        ))
                    }
                }
                _ => Err(SimplifyError::Parse(format!(
                    "unknown operator `{head_atom}`"
                ))),
            }
        }
        Sexp::Empty => Err(SimplifyError::Parse("empty expression".into())),
    }
}

fn parse_nary<F>(op: &str, args: &[Sexp], ctor: F) -> Result<Expr, SimplifyError>
where
    F: Fn(Vec<Expr>) -> Expr,
{
    if args.is_empty() {
        return Err(SimplifyError::Parse(format!(
            "`{op}` needs at least 1 argument"
        )));
    }
    let mut items = Vec::new();
    for a in args {
        items.push(sexp_to_expr(a)?);
    }
    Ok(ctor(items))
}

fn parse_div(args: &[Sexp]) -> Result<Expr, SimplifyError> {
    if args.len() < 2 {
        return Err(SimplifyError::Parse(
            "`/` needs at least 2 arguments".into(),
        ));
    }
    let mut exprs = Vec::new();
    for a in args {
        exprs.push(sexp_to_expr(a)?);
    }
    let mut result = exprs.remove(0);
    for denom in exprs {
        result = Expr::Mul(vec![result, Expr::Pow(Box::new(denom), -1)]);
    }
    Ok(result)
}

fn parse_pow(args: &[Sexp]) -> Result<Expr, SimplifyError> {
    if args.len() != 2 {
        return Err(SimplifyError::Parse("`^` needs base and exponent".into()));
    }
    let base = sexp_to_expr(&args[0])?;
    let exp_expr = sexp_to_expr(&args[1])?;
    let exp = match exp_expr {
        Expr::Const(r) if r.is_integer() => {
            let n = *r.numer();
            if let Ok(v) = i32::try_from(n) {
                v
            } else {
                return Err(SimplifyError::Parse("exponent out of range for i32".into()));
            }
        }
        _ => {
            return Err(SimplifyError::Parse(
                "exponent must be an integer constant".into(),
            ))
        }
    };
    Ok(Expr::Pow(Box::new(base), exp))
}

fn parse_neg(args: &[Sexp]) -> Result<Expr, SimplifyError> {
    if args.len() != 1 {
        return Err(SimplifyError::Parse(
            "`neg` needs exactly 1 argument".into(),
        ));
    }
    Ok(Expr::Neg(Box::new(sexp_to_expr(&args[0])?)))
}

fn parse_atom(atom: &str) -> Option<Expr> {
    if let Some(r) = parse_rational(atom) {
        return Some(Expr::Const(r));
    }
    if let Ok(v) = atom.parse::<i64>() {
        return Some(Expr::Const(Rational64::from_integer(v)));
    }
    if is_symbol(atom) {
        return Some(Expr::Symbol(atom.to_string()));
    }
    None
}

fn parse_rational(atom: &str) -> Option<Rational64> {
    let parts: Vec<&str> = atom.split('/').collect();
    if parts.len() == 2 {
        let numer = parts[0].parse::<i64>().ok()?;
        let denom = parts[1].parse::<i64>().ok()?;
        if denom == 0 {
            return None;
        }
        return Some(Rational64::new(numer, denom));
    }
    None
}

fn is_symbol(atom: &str) -> bool {
    atom.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '?')
        && !atom.is_empty()
}

// ======================
// Normalization
// ======================

pub fn normalize(expr: Expr) -> Expr {
    match expr {
        Expr::Const(c) => Expr::Const(c),
        Expr::Symbol(s) => Expr::Symbol(s),
        Expr::Neg(inner) => normalize_neg(*inner),
        Expr::Pow(base, exp) => normalize_pow(*base, exp),
        Expr::Add(items) => normalize_add(items),
        Expr::Mul(items) => normalize_mul(items),
    }
}

fn normalize_neg(expr: Expr) -> Expr {
    let inner = normalize(expr);
    match inner {
        Expr::Const(c) => Expr::Const(-c),
        Expr::Neg(x) => *x,
        Expr::Mul(mut factors) => {
            factors.insert(0, Expr::Const(Rational64::from_integer(-1)));
            normalize_mul(factors)
        }
        other => Expr::Mul(vec![Expr::Const(Rational64::from_integer(-1)), other]),
    }
}

fn normalize_pow(base: Expr, exp: i32) -> Expr {
    let base = normalize(base);
    if exp == 0 {
        return Expr::Const(Rational64::one());
    }
    if exp == 1 {
        return base;
    }

    match base {
        Expr::Const(c) => {
            if c.is_zero() && exp < 0 {
                return Expr::Pow(Box::new(Expr::Const(c)), exp);
            }
            Expr::Const(c.pow(exp))
        }
        Expr::Neg(inner) => {
            if exp % 2 == 0 {
                Expr::Pow(inner, exp)
            } else {
                normalize_neg(Expr::Pow(inner, exp))
            }
        }
        Expr::Pow(inner, e) => {
            if let Some(new_exp) = e.checked_mul(exp) {
                normalize_pow(*inner, new_exp)
            } else {
                Expr::Pow(Box::new(Expr::Pow(inner, e)), exp)
            }
        }
        other => Expr::Pow(Box::new(other), exp),
    }
}

fn normalize_add(items: Vec<Expr>) -> Expr {
    let mut terms = Vec::new();
    let mut const_sum = Rational64::zero();
    for item in items {
        let norm = normalize(item);
        match norm {
            Expr::Add(inner) => {
                for t in inner {
                    terms.push(t);
                }
            }
            Expr::Const(c) => const_sum += c,
            Expr::Neg(inner) => {
                terms.push(normalize_neg(*inner));
            }
            other => terms.push(other),
        }
    }

    if !const_sum.is_zero() {
        terms.push(Expr::Const(const_sum));
    }

    terms.retain(|e| !e.is_const_zero());
    if terms.is_empty() {
        return Expr::Const(Rational64::zero());
    }
    if terms.len() == 1 {
        return terms.pop().unwrap();
    }

    sort_terms(&mut terms);
    Expr::Add(terms)
}

fn normalize_mul(items: Vec<Expr>) -> Expr {
    let mut factors = Vec::new();
    let mut const_prod = Rational64::one();

    for item in items {
        let norm = normalize(item);
        match norm {
            Expr::Const(c) => const_prod *= c,
            Expr::Mul(inner) => {
                for f in inner {
                    factors.push(f);
                }
            }
            Expr::Neg(inner) => {
                const_prod = -const_prod;
                factors.push(*inner);
            }
            other => factors.push(other),
        }
    }

    if const_prod.is_zero() {
        return Expr::Const(Rational64::zero());
    }

    // Pow combination
    let mut base_map: BTreeMap<String, (Expr, i32)> = BTreeMap::new();
    for factor in factors {
        let (base, exp) = match factor {
            Expr::Pow(b, e) => (*b, e),
            other => (other, 1),
        };
        let key = to_canonical_string(&base);
        let entry = base_map.entry(key).or_insert((base, 0));
        entry.1 = entry.1.checked_add(exp).unwrap_or(entry.1);
    }

    let mut final_factors = Vec::new();
    for (_k, (base, exp)) in base_map {
        if exp == 0 {
            continue;
        }
        if exp == 1 {
            final_factors.push(base);
        } else {
            final_factors.push(normalize_pow(base, exp));
        }
    }

    if const_prod.is_one() {
        // nothing
    } else {
        final_factors.push(Expr::Const(const_prod));
    }

    if final_factors.is_empty() {
        return Expr::Const(const_prod);
    }

    sort_terms(&mut final_factors);

    if final_factors.len() == 1 {
        return final_factors.pop().unwrap();
    }

    Expr::Mul(final_factors)
}

fn sort_terms(items: &mut [Expr]) {
    items.sort_by_key(term_key);
}

fn term_key(expr: &Expr) -> (u8, String) {
    match expr {
        Expr::Const(_) => (0, to_canonical_string(expr)),
        _ => (1, to_canonical_string(expr)),
    }
}

// ======================
// Canonical formatting
// ======================

pub fn to_canonical_string(expr: &Expr) -> String {
    match expr {
        Expr::Const(c) => format_rational(*c),
        Expr::Symbol(s) => s.clone(),
        Expr::Neg(inner) => format!("(neg {})", to_canonical_string(inner)),
        Expr::Pow(base, exp) => format!("(^ {} {})", to_canonical_string(base), exp),
        Expr::Add(items) => format!("(+ {})", items.iter().map(to_canonical_string).join(" ")),
        Expr::Mul(items) => format!("(* {})", items.iter().map(to_canonical_string).join(" ")),
    }
}

fn format_rational(r: Rational64) -> String {
    if *r.denom() == 1 {
        format!("{}", r.numer())
    } else {
        format!("{}/{}", r.numer(), r.denom())
    }
}

// ======================
// Egg language + lowering/lifting
// ======================

define_language! {
    pub enum Lang {
        Num(Rational64),
        Symbol(Symbol),
        "+" = Add([Id; 2]),
        "*" = Mul([Id; 2]),
        "^" = Pow([Id; 2]),
        "neg" = Neg(Id),
    }
}

#[derive(Default)]
struct ConstFold;
type EGraphCF = EGraph<Lang, ConstFold>;

impl Analysis<Lang> for ConstFold {
    type Data = Option<Rational64>;

    fn make(egraph: &EGraphCF, enode: &Lang) -> Self::Data {
        match enode {
            Lang::Num(n) => Some(*n),
            Lang::Add([a, b]) => Some(egraph[*a].data? + egraph[*b].data?),
            Lang::Mul([a, b]) => Some(egraph[*a].data? * egraph[*b].data?),
            Lang::Neg(a) => Some(-egraph[*a].data?),
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
            let const_id = egraph.add(Lang::Num(c));
            egraph.union(id, const_id);
        }
    }
}

fn rules() -> Vec<Rewrite<Lang, ConstFold>> {
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

pub fn lower_to_egg(expr: &Expr) -> Result<RecExpr<Lang>, SimplifyError> {
    let mut builder = RecExpr::default();
    let id = lower_expr(expr, &mut builder)?;
    assert_eq!(id, Id::from(builder.as_ref().len() - 1));
    Ok(builder)
}

fn lower_expr(expr: &Expr, builder: &mut RecExpr<Lang>) -> Result<Id, SimplifyError> {
    match expr {
        Expr::Const(c) => Ok(builder.add(Lang::Num(*c))),
        Expr::Symbol(s) => Ok(builder.add(Lang::Symbol(Symbol::from(s.as_str())))),
        Expr::Neg(inner) => {
            let child = lower_expr(inner, builder)?;
            Ok(builder.add(Lang::Neg(child)))
        }
        Expr::Pow(base, exp) => {
            let base_id = lower_expr(base, builder)?;
            let exp_id = builder.add(Lang::Num(Rational64::from_integer(*exp as i64)));
            Ok(builder.add(Lang::Pow([base_id, exp_id])))
        }
        Expr::Add(items) => lower_nary(items, builder, |a, b| Lang::Add([a, b])),
        Expr::Mul(items) => lower_nary(items, builder, |a, b| Lang::Mul([a, b])),
    }
}

fn lower_nary<F>(items: &[Expr], builder: &mut RecExpr<Lang>, make: F) -> Result<Id, SimplifyError>
where
    F: Fn(Id, Id) -> Lang,
{
    if items.is_empty() {
        return Err(SimplifyError::Parse("n-ary op missing arguments".into()));
    }
    if items.len() == 1 {
        return lower_expr(&items[0], builder);
    }
    // right associative to be deterministic
    let mut iter = items.iter().rev();
    let mut acc = lower_expr(iter.next().unwrap(), builder)?;
    for item in iter {
        let left = lower_expr(item, builder)?;
        acc = builder.add(make(left, acc));
    }
    Ok(acc)
}

pub fn lift_from_egg(expr: &RecExpr<Lang>) -> Expr {
    let root = Id::from(expr.as_ref().len() - 1);
    lift(expr, root)
}

fn lift(rec: &RecExpr<Lang>, id: Id) -> Expr {
    match &rec[id] {
        Lang::Num(n) => Expr::Const(*n),
        Lang::Symbol(sym) => Expr::Symbol(sym.to_string()),
        Lang::Neg(a) => Expr::Neg(Box::new(lift(rec, *a))),
        Lang::Add([a, b]) => Expr::Add(vec![lift(rec, *a), lift(rec, *b)]),
        Lang::Mul([a, b]) => Expr::Mul(vec![lift(rec, *a), lift(rec, *b)]),
        Lang::Pow([a, b]) => {
            let exp = match lift(rec, *b) {
                Expr::Const(r) if r.is_integer() => i32::try_from(*r.numer()).unwrap_or(0),
                other => {
                    panic!("non-integer exponent after extraction: {:?}", other);
                }
            };
            Expr::Pow(Box::new(lift(rec, *a)), exp)
        }
    }
}

// ======================
// Tests
// ======================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn parse_norm(input: &str) -> Expr {
        normalize(parse_sexpr(input).unwrap())
    }

    fn canon(input: &str) -> String {
        to_canonical_string(&parse_norm(input))
    }

    fn eval(expr: &Expr, env: &HashMap<String, Rational64>) -> Option<Rational64> {
        match expr {
            Expr::Const(c) => Some(*c),
            Expr::Symbol(s) => env.get(s).copied(),
            Expr::Neg(inner) => eval(inner, env).map(|v| -v),
            Expr::Pow(base, exp) => {
                let b = eval(base, env)?;
                if *exp < 0 && b.is_zero() {
                    return None;
                }
                Some(b.pow(*exp))
            }
            Expr::Add(items) => {
                let mut acc = Rational64::zero();
                for it in items {
                    acc += eval(it, env)?;
                }
                Some(acc)
            }
            Expr::Mul(items) => {
                let mut acc = Rational64::one();
                for it in items {
                    acc *= eval(it, env)?;
                }
                Some(acc)
            }
        }
    }

    #[test]
    fn parse_nary_add() {
        assert_eq!(canon("(+ x y 0 3 x)"), "(+ 3 x x y)");
    }

    #[test]
    fn parse_div_rational() {
        assert_eq!(canon("(/ 1 2)"), "1/2");
    }

    #[test]
    fn parse_div_nary_left_assoc() {
        let expr = parse_norm("(/ x y z)");
        assert_eq!(to_canonical_string(&expr), "(* (^ y -1) (^ z -1) x)");
    }

    #[test]
    fn pow_simplify_and_merge() {
        assert_eq!(canon("(^ x 0)"), "1");
        assert_eq!(canon("(^ (^ x 2) 3)"), "(^ x 6)");
        assert_eq!(canon("(* (^ x 2) (^ x 3))"), "(^ x 5)");
    }

    #[test]
    fn neg_handling() {
        assert_eq!(canon("(neg (neg x))"), "x");
        assert_eq!(canon("(* (neg x) (neg y))"), "(* x y)");
    }

    #[test]
    fn eval_equivalence_examples() {
        let env: HashMap<_, _> = [
            ("x".to_string(), Rational64::new(1, 2)),
            ("y".to_string(), Rational64::new(3, 2)),
        ]
        .into();
        let input = "(+ x y 0 3 x)";
        let norm = parse_norm(input);
        let lowered = lower_to_egg(&norm).unwrap();
        let runner = build_runner(lowered, 10);
        let best = extract_best(&runner);
        let simplified = normalize(lift_from_egg(&best));
        let eval_in = eval(&norm, &env).unwrap();
        let eval_out = eval(&simplified, &env).unwrap();
        assert_eq!(eval_in, eval_out);
    }
}
