# Canonical Normal Form Specification

Appendix A. Canonical Normal Form for Symbolic Expressions

## A.1 Scope and Design Goal

We define a canonical normal form for a class of symbolic algebraic
expressions, serving as a deterministic front-end for equality reasoning,
e-graph rewriting, and subsequent symbolic constructions (e.g. polylogarithms
and symbols).

The design goals are:

1. Uniqueness: syntactically equivalent expressions reduce to a unique
   representation.
2. Stability: normalization is deterministic and idempotent.
3. Compactness: algebraic redundancies are eliminated as early as possible.
4. Composability: the output form minimizes combinatorial blow-up when embedded
   into e-graphs.
5. Extensibility: the form admits later extensions (e.g. logarithms,
   polylogarithms) without redesign.

The current specification (v0.1.1) covers rational functions with integer
powers.

## A.2 Expression Language

Expressions are parsed from an S-expression syntax and normalized into the
following abstract syntax:

- Rational constants (Q) represented exactly
- Variables: symbols (x, y, z, ...)
- N-ary addition: Add(Vec)
- N-ary multiplication: Mul(Vec)
- Integer powers: Pow(base, exp) with exp in Z
- Unary negation (syntax-level only; absorbed during normalization)

Division is treated as syntactic sugar and is eliminated during normalization.

## A.3 Canonicalization Rules

### A.3.1 Structural Flattening

Nested additions and multiplications are flattened:

- a + (b + c) -> a + b + c
- a * (b * c) -> a * b * c

### A.3.2 Identity and Annihilator Rules

- Additive identity:
  - a + 0 -> a
  - empty sum -> 0
- Multiplicative identity:
  - a * 1 -> a
  - empty product -> 1
- Multiplicative annihilator:
  - a * 0 * b -> 0

The annihilator rule is strict: the presence of a single zero factor collapses
the entire product.

### A.3.3 Rational Folding

All rational constants in an Add or Mul node are combined:

- In addition: summed into a single rational.
- In multiplication: multiplied into a single rational.

Rationals are stored in reduced form.

### A.3.4 Division Elimination

Division does not appear in canonical form. It is desugared as:

- (/ a b c ...) -> (* a (^ b -1) (^ c -1) ...)

and then normalized via the multiplication and power rules.

### A.3.5 Power Normalization

Integer powers obey:

- x^0 = 1 (including the convention 0^0 = 1)
- x^1 = x
- (x^a)^b -> x^(a*b) when integer multiplication is defined
- Rational powers are folded when well-defined:
  - (c)^n in Q, n in Z

If a negative exponent is applied to zero, the expression is left as Pow(0, n)
without further simplification.

### A.3.6 Power Merging in Products

Within a product, powers with the same base are merged:

- x^a * x^b -> x^(a+b)
- x * x^a -> x^(a+1)

This rule is central to controlling expression growth.

### A.3.7 Sign Normalization

Negation is normalized to avoid structural ambiguity:

- Double negation is eliminated: -(-x) = x.
- Signs are absorbed into the rational coefficient whenever possible.
- Products should not contain explicit negated subterms when an equivalent
  rational prefactor exists.

### A.3.8 Ordering and Printing

For both addition and multiplication:

1. Rational constants appear first.
2. Remaining terms are sorted lexicographically by their canonical string.

This ordering guarantees a unique textual representation.

## A.4 Canonical Output Guarantee

The normalization function N satisfies:

- Idempotence: N(N(E)) = N(E).
- Semantic preservation: N(E) is equivalent to E under rational arithmetic.
- Stability: repeated normalization yields the same string.

Note: The current v0.1.1 rules are not a complete decision procedure for
algebraic equality. They define a deterministic normal form for the supported
syntax and rewrite rules.
