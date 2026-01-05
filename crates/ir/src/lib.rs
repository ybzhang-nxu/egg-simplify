use num_rational::Rational64;
use num_traits::{One, Zero};
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Rational(Rational64),
    Var(String),
    Add(Vec<Expr>),
    Mul(Vec<Expr>),
    Neg(Box<Expr>),
    Pow(Box<Expr>, i32),
    Log(Box<Expr>),
    Li2(Box<Expr>),
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("unexpected end of input")]
    Eof,
    #[error("unexpected token: {0}")]
    UnexpectedToken(String),
    #[error("expected operator after '('")]
    ExpectedOperator,
    #[error("expected ')'")]
    ExpectedRParen,
    #[error("unknown operator '{0}'")]
    UnknownOperator(String),
    #[error("invalid number '{0}'")]
    InvalidNumber(String),
    #[error("'-' expects exactly one argument")]
    InvalidUnaryMinus,
    #[error("'^' expects exactly two arguments")]
    InvalidPowArity,
    #[error("expected integer exponent")]
    ExpectedExponent,
    #[error("invalid exponent '{0}'")]
    InvalidExponent(String),
    #[error("'/' expects at least two arguments")]
    InvalidDivisionArity,
    #[error("'log' expects exactly one argument")]
    InvalidLogArity,
    #[error("'li2' expects exactly one argument")]
    InvalidLi2Arity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    LParen,
    RParen,
    Atom(String),
}

impl Token {
    fn describe(&self) -> String {
        match self {
            Token::LParen => "(".to_string(),
            Token::RParen => ")".to_string(),
            Token::Atom(atom) => atom.clone(),
        }
    }
}

pub fn parse_sexpr(input: &str) -> Result<Expr, ParseError> {
    let tokens = tokenize(input);
    let mut index = 0;
    let expr = parse_expr(&tokens, &mut index)?;
    if index != tokens.len() {
        return Err(ParseError::UnexpectedToken(tokens[index].describe()));
    }
    Ok(expr)
}

fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '(' => tokens.push(Token::LParen),
            ')' => tokens.push(Token::RParen),
            ch if ch.is_whitespace() => {
                continue;
            }
            _ => {
                let mut atom = String::new();
                atom.push(ch);
                while let Some(next) = chars.peek() {
                    if next.is_whitespace() || *next == '(' || *next == ')' {
                        break;
                    }
                    atom.push(*next);
                    chars.next();
                }
                tokens.push(Token::Atom(atom));
            }
        }
    }
    tokens
}

fn parse_expr(tokens: &[Token], index: &mut usize) -> Result<Expr, ParseError> {
    let token = tokens.get(*index).ok_or(ParseError::Eof)?;
    match token {
        Token::Atom(atom) => {
            *index += 1;
            parse_atom(atom)
        }
        Token::LParen => {
            *index += 1;
            parse_list(tokens, index)
        }
        Token::RParen => Err(ParseError::UnexpectedToken(token.describe())),
    }
}

fn parse_list(tokens: &[Token], index: &mut usize) -> Result<Expr, ParseError> {
    let op = match tokens.get(*index) {
        Some(Token::Atom(atom)) => {
            *index += 1;
            atom.clone()
        }
        Some(token) => return Err(ParseError::UnexpectedToken(token.describe())),
        None => return Err(ParseError::ExpectedOperator),
    };

    match op.as_str() {
        "^" => parse_pow_list(tokens, index),
        "/" => parse_div_list(tokens, index),
        _ => {
            let mut args = Vec::new();
            loop {
                match tokens.get(*index) {
                    Some(Token::RParen) => {
                        *index += 1;
                        break;
                    }
                    Some(_) => args.push(parse_expr(tokens, index)?),
                    None => return Err(ParseError::ExpectedRParen),
                }
            }

            match op.as_str() {
                "+" => Ok(Expr::Add(args)),
                "*" => Ok(Expr::Mul(args)),
                "-" => {
                    if args.len() == 1 {
                        Ok(Expr::Neg(Box::new(args.into_iter().next().unwrap())))
                    } else {
                        Err(ParseError::InvalidUnaryMinus)
                    }
                }
                "log" => {
                    if args.len() == 1 {
                        Ok(Expr::Log(Box::new(args.into_iter().next().unwrap())))
                    } else {
                        Err(ParseError::InvalidLogArity)
                    }
                }
                "li2" => {
                    if args.len() == 1 {
                        Ok(Expr::Li2(Box::new(args.into_iter().next().unwrap())))
                    } else {
                        Err(ParseError::InvalidLi2Arity)
                    }
                }
                _ => Err(ParseError::UnknownOperator(op)),
            }
        }
    }
}

fn parse_pow_list(tokens: &[Token], index: &mut usize) -> Result<Expr, ParseError> {
    let base = parse_expr(tokens, index)?;
    let exp_token = tokens.get(*index).ok_or(ParseError::ExpectedExponent)?;
    let exp = match exp_token {
        Token::Atom(atom) => parse_exponent(atom)?,
        _ => return Err(ParseError::ExpectedExponent),
    };
    *index += 1;

    match tokens.get(*index) {
        Some(Token::RParen) => {
            *index += 1;
            Ok(Expr::Pow(Box::new(base), exp))
        }
        Some(_) => Err(ParseError::InvalidPowArity),
        None => Err(ParseError::ExpectedRParen),
    }
}

fn parse_div_list(tokens: &[Token], index: &mut usize) -> Result<Expr, ParseError> {
    let mut args = Vec::new();
    loop {
        match tokens.get(*index) {
            Some(Token::RParen) => {
                *index += 1;
                break;
            }
            Some(_) => args.push(parse_expr(tokens, index)?),
            None => return Err(ParseError::ExpectedRParen),
        }
    }

    if args.len() < 2 {
        return Err(ParseError::InvalidDivisionArity);
    }

    let mut factors = Vec::with_capacity(args.len());
    factors.push(args.remove(0));
    for denom in args {
        factors.push(Expr::Pow(Box::new(denom), -1));
    }
    Ok(Expr::Mul(factors))
}

fn parse_exponent(atom: &str) -> Result<i32, ParseError> {
    atom.parse::<i32>()
        .map_err(|_| ParseError::InvalidExponent(atom.to_string()))
}

fn parse_atom(atom: &str) -> Result<Expr, ParseError> {
    if let Some((num_str, denom_str)) = atom.split_once('/') {
        let num: i64 = num_str
            .parse()
            .map_err(|_| ParseError::InvalidNumber(atom.to_string()))?;
        let denom: i64 = denom_str
            .parse()
            .map_err(|_| ParseError::InvalidNumber(atom.to_string()))?;
        if denom == 0 {
            return Err(ParseError::InvalidNumber(atom.to_string()));
        }
        return Ok(Expr::Rational(Rational64::new(num, denom)));
    }

    if let Ok(value) = atom.parse::<i64>() {
        return Ok(Expr::Rational(Rational64::from_integer(value)));
    }

    Ok(Expr::Var(atom.to_string()))
}

impl Expr {
    pub fn normalize(&self) -> Expr {
        match self {
            Expr::Rational(value) => Expr::Rational(*value),
            Expr::Var(name) => Expr::Var(name.clone()),
            Expr::Neg(inner) => {
                let normalized = inner.normalize();
                match normalized {
                    Expr::Rational(value) => Expr::Rational(-value),
                    Expr::Neg(inner_again) => *inner_again,
                    other => Expr::Neg(Box::new(other)),
                }
            }
            Expr::Add(children) => normalize_add(children),
            Expr::Mul(children) => normalize_mul(children),
            Expr::Pow(base, exp) => normalize_pow(base, *exp),
            Expr::Log(inner) => Expr::Log(Box::new(inner.normalize())),
            Expr::Li2(inner) => Expr::Li2(Box::new(inner.normalize())),
        }
    }

    pub fn to_canonical_string(&self) -> String {
        match self {
            Expr::Rational(value) => format_rational(value),
            Expr::Var(name) => name.clone(),
            Expr::Neg(inner) => format!("(- {})", inner.to_canonical_string()),
            Expr::Add(children) => format_list("+", children),
            Expr::Mul(children) => format_list("*", children),
            Expr::Pow(base, exp) => format!("(^ {} {exp})", base.to_canonical_string()),
            Expr::Log(inner) => format!("(log {})", inner.to_canonical_string()),
            Expr::Li2(inner) => format!("(li2 {})", inner.to_canonical_string()),
        }
    }
}

fn normalize_add(children: &[Expr]) -> Expr {
    let mut flat = Vec::new();
    let mut sum = Rational64::zero();

    for child in children {
        let normalized = child.normalize();
        match normalized {
            Expr::Add(grand_children) => {
                for grand_child in grand_children {
                    match grand_child {
                        Expr::Rational(value) => sum += value,
                        other => flat.push(other),
                    }
                }
            }
            Expr::Rational(value) => sum += value,
            other => flat.push(other),
        }
    }

    if !sum.is_zero() {
        flat.push(Expr::Rational(sum));
    }

    if flat.is_empty() {
        return Expr::Rational(Rational64::zero());
    }

    let sorted = sort_children(flat);
    if sorted.len() == 1 {
        sorted.into_iter().next().unwrap()
    } else {
        Expr::Add(sorted)
    }
}

fn normalize_mul(children: &[Expr]) -> Expr {
    let mut product = Rational64::one();
    let mut pow_map: std::collections::HashMap<String, (Expr, i32)> =
        std::collections::HashMap::new();
    let mut overflow_factors = Vec::new();

    let mut stack: Vec<Expr> = children.iter().map(|child| child.normalize()).collect();
    while let Some(expr) = stack.pop() {
        match expr {
            Expr::Mul(grand_children) => {
                for grand_child in grand_children {
                    stack.push(grand_child);
                }
            }
            Expr::Rational(value) => {
                product *= value;
                if product.is_zero() {
                    return Expr::Rational(Rational64::zero());
                }
            }
            Expr::Neg(inner) => {
                product *= Rational64::from_integer(-1);
                stack.push(*inner);
            }
            Expr::Pow(base, exp) => {
                accumulate_pow(*base, exp, &mut pow_map, &mut overflow_factors);
            }
            other => {
                accumulate_pow(other, 1, &mut pow_map, &mut overflow_factors);
            }
        }
    }

    if product.is_zero() {
        return Expr::Rational(Rational64::zero());
    }

    let mut factors = Vec::new();
    for (_, (base, exp)) in pow_map {
        if exp == 0 {
            continue;
        }
        if exp == 1 {
            factors.push(base);
        } else {
            factors.push(Expr::Pow(Box::new(base), exp));
        }
    }
    factors.extend(overflow_factors);

    if product != Rational64::one() || factors.is_empty() {
        factors.push(Expr::Rational(product));
    }

    let sorted = sort_children(factors);
    if sorted.len() == 1 {
        sorted.into_iter().next().unwrap()
    } else {
        Expr::Mul(sorted)
    }
}

fn accumulate_pow(
    base: Expr,
    exp: i32,
    pow_map: &mut std::collections::HashMap<String, (Expr, i32)>,
    overflow: &mut Vec<Expr>,
) {
    if exp == 0 {
        return;
    }
    let key = base.to_canonical_string();
    if let Some((_, existing_exp)) = pow_map.get_mut(&key) {
        if let Some(sum) = existing_exp.checked_add(exp) {
            if sum == 0 {
                pow_map.remove(&key);
            } else {
                *existing_exp = sum;
            }
        } else {
            overflow.push(Expr::Pow(Box::new(base), exp));
        }
        return;
    }
    pow_map.insert(key, (base, exp));
}

fn normalize_pow(base: &Expr, exp: i32) -> Expr {
    let base_norm = base.normalize();
    if exp == 0 {
        return Expr::Rational(Rational64::one());
    }
    if exp == 1 {
        return base_norm;
    }

    match base_norm {
        Expr::Pow(inner, inner_exp) => {
            if let Some(combined) = inner_exp.checked_mul(exp) {
                return normalize_pow(&inner, combined);
            }
            Expr::Pow(Box::new(Expr::Pow(inner, inner_exp)), exp)
        }
        Expr::Rational(value) => {
            if let Some(result) = pow_rational(value, exp) {
                return Expr::Rational(result);
            }
            Expr::Pow(Box::new(Expr::Rational(value)), exp)
        }
        Expr::Neg(inner) => {
            if exp % 2 == 0 {
                return normalize_pow(&inner, exp);
            }
            let inner_pow = normalize_pow(&inner, exp);
            Expr::Neg(Box::new(inner_pow))
        }
        other => Expr::Pow(Box::new(other), exp),
    }
}

fn pow_rational(value: Rational64, exp: i32) -> Option<Rational64> {
    if exp == 0 {
        return Some(Rational64::one());
    }
    if exp == i32::MIN {
        return None;
    }
    let numer = *value.numer();
    let denom = *value.denom();
    let exp_abs = exp.unsigned_abs();
    if exp < 0 && numer == 0 {
        return None;
    }
    let (base_numer, base_denom) = if exp >= 0 {
        (numer, denom)
    } else {
        (denom, numer)
    };
    let numer_pow = base_numer.checked_pow(exp_abs)?;
    let denom_pow = base_denom.checked_pow(exp_abs)?;
    Some(Rational64::new(numer_pow, denom_pow))
}

fn sort_children(children: Vec<Expr>) -> Vec<Expr> {
    let mut keyed: Vec<((u8, String), usize, Expr)> = children
        .into_iter()
        .enumerate()
        .map(|(index, expr)| {
            let key = match &expr {
                Expr::Rational(_) => (0, expr.to_canonical_string()),
                _ => (1, expr.to_canonical_string()),
            };
            (key, index, expr)
        })
        .collect();
    keyed.sort_by(|a, b| {
        a.0 .0
            .cmp(&b.0 .0)
            .then(a.0 .1.cmp(&b.0 .1))
            .then(a.1.cmp(&b.1))
    });
    keyed.into_iter().map(|(_, _, expr)| expr).collect()
}

fn format_list(op: &str, children: &[Expr]) -> String {
    if children.is_empty() {
        format!("({op})")
    } else {
        let rendered = children
            .iter()
            .map(|child| child.to_canonical_string())
            .collect::<Vec<_>>()
            .join(" ");
        format!("({op} {rendered})")
    }
}

fn format_rational(value: &Rational64) -> String {
    let numer = *value.numer();
    let denom = *value.denom();
    if denom == 1 {
        numer.to_string()
    } else {
        format!("{numer}/{denom}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canon(input: &str) -> String {
        let expr = parse_sexpr(input).expect("parse");
        expr.normalize().to_canonical_string()
    }

    #[test]
    fn normalize_nary_add_basic() {
        assert_eq!(canon("(+ x y 0 3 x)"), "(+ 3 x x y)");
    }

    #[test]
    fn normalize_nary_mul_basic() {
        assert_eq!(canon("(* y x 2 1 z)"), "(* 2 x y z)");
    }

    #[test]
    fn parse_rational_literal() {
        assert_eq!(canon("1/2"), "1/2");
        assert_eq!(canon("-7/3"), "-7/3");
    }

    #[test]
    fn normalize_div_basic() {
        assert_eq!(canon("(/ 1 2)"), "1/2");
    }

    #[test]
    fn normalize_div_nary() {
        assert_eq!(canon("(/ x y z)"), "(* (^ y -1) (^ z -1) x)");
    }

    #[test]
    fn normalize_div_mul_simplifies() {
        assert_eq!(canon("(* 2 (/ 1 2))"), "1");
    }

    #[test]
    fn normalize_pow_basic() {
        assert_eq!(canon("(^ x 0)"), "1");
        assert_eq!(canon("(^ x 1)"), "x");
    }

    #[test]
    fn normalize_pow_nesting() {
        assert_eq!(canon("(^ (^ x 2) 3)"), "(^ x 6)");
    }

    #[test]
    fn normalize_pow_combine_mul() {
        assert_eq!(canon("(* (^ x 2) (^ x 3))"), "(^ x 5)");
        assert_eq!(canon("(* x (^ x 3))"), "(^ x 4)");
    }

    #[test]
    fn normalize_pow_negative_exponent() {
        assert_eq!(canon("(^ 2 -1)"), "1/2");
        assert_eq!(canon("(^ 1/2 -2)"), "4");
    }

    #[test]
    fn mul_zero_absorbs_all() {
        let e = parse_sexpr("(* x 0 y)").unwrap().normalize();
        assert_eq!(e.to_canonical_string(), "0");
    }

    #[test]
    fn mul_zero_nested() {
        let e = parse_sexpr("(* x (* y 0 z))").unwrap().normalize();
        assert_eq!(e.to_canonical_string(), "0");
    }

    #[test]
    fn mul_zero_constant() {
        let e = parse_sexpr("(* 3 0)").unwrap().normalize();
        assert_eq!(e.to_canonical_string(), "0");
    }

    #[test]
    fn pow_zero_negative_exponent_kept() {
        assert_eq!(canon("(^ 0 -1)"), "(^ 0 -1)");
    }

    #[test]
    fn negation_basic() {
        assert_eq!(canon("(- (- x))"), "x");
    }

    #[test]
    fn normalize_roundtrip_is_stable() {
        let input = "(+ x (+ y 0) (+ 3 x))";
        let printed = canon(input);
        let printed_again = canon(&printed);
        assert_eq!(printed, printed_again);
    }

    #[test]
    fn normalize_roundtrip_log_is_stable() {
        let input = "(log (+ x 0))";
        let printed = canon(input);
        let printed_again = canon(&printed);
        assert_eq!(printed, printed_again);
    }

    #[test]
    fn normalize_roundtrip_li2_is_stable() {
        let input = "(li2 (+ x 0))";
        let printed = canon(input);
        let printed_again = canon(&printed);
        assert_eq!(printed, printed_again);
    }

    #[test]
    fn normalize_example_add() {
        assert_eq!(canon("(+ x (+ y 0) (+ 3 x))"), "(+ 3 x x y)");
    }
}
