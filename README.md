# mpl-simplifier v0.1.2

Minimal Rust workspace skeleton for an MPL simplifier. v0.1.2 keeps the
v0.1.1 canonical normalization and adds a minimal symbol layer (Log/Li2,
symbolization, and weight-2 integrability checks) plus CLI subcommands.

## Workspace layout
- `crates/ir`: AST, parser, normalization, canonical printing.
- `crates/verify`: rational evaluation and sample equivalence helpers.
- `crates/rewrite`: e-graph rewrite engine (legacy, placeholder for future).
- `crates/symbol`: symbol-level extensions (symbol generation and integrability checks).
- `crates/cli`: `mpl-simplify` command-line interface.

## Expression language (v0.1.2)
- S-expression syntax.
- Operators:
  - N-ary addition: `(+ a b c ...)`
  - N-ary multiplication: `(* a b c ...)`
  - N-ary division: `(/ a b c ...)` (desugars to multiplication by inverse)
  - Integer power: `(^ base exp)` where `exp` is an integer atom (can be
    negative).
  - Unary negation: `(- x)` (exactly one argument).
  - Unary log: `(log x)` (parsed/printed only; no functional identities).
  - Unary dilog: `(li2 x)` (parsed/printed only; no functional identities).
- Numbers: integers or rationals like `1/2` or `-7/3`.

## Canonical normalization (v0.1.1; unchanged in v0.1.2)
Normalization is deterministic and idempotent. The canonical form is designed
to be stable and compact for e-graph use.

- Structural flattening:
  - `(+ a (+ b c)) -> (+ a b c)`
  - `(* a (* b c)) -> (* a b c)`
- Identities and annihilators:
  - `(+ x 0) -> x`, empty sum -> `0`
  - `(* x 1) -> x`, empty product -> `1`
  - `(* x 0 y) -> 0` (strict annihilator)
- Rational folding:
  - All rational constants in `Add` and `Mul` are combined into one value.
- Division elimination:
  - `(/ a b c)` -> `(* a (^ b -1) (^ c -1))`
  - Canonical output never contains `/`.
- Power rules:
  - `x^0 -> 1` (including `0^0 -> 1`)
  - `x^1 -> x`
  - `(x^a)^b -> x^(a*b)` when `a*b` fits in `i32`
  - Rational powers are folded when safe (e.g. `(^ 2 -1) -> 1/2`)
  - `(^ 0 -1)` is kept as `(^ 0 -1)` (no panic)
- Power merging in products:
  - `x^a * x^b -> x^(a+b)`
  - `x * x^a -> x^(a+1)`
- Sign normalization:
  - `(- (- x)) -> x`
  - In `Mul`, negative signs are absorbed into the rational coefficient.
- Ordering and printing:
  - In `Add`/`Mul`, rational constants appear first.
  - Remaining children are sorted by canonical string.
  - Powers print as `(^ base exp)`.
  - `Log`/`Li2` only normalize their argument (no identity rules).

For a formal specification, see `docs/canonical_form.md`.

## CLI
```bash
cargo run -p mpl-simplify -- normalize --expr "(+ x 0)"
cargo run -p mpl-simplify -- version
cargo run -p mpl-simplify -- symbol --expr "(log x)"
cargo run -p mpl-simplify -- check-integrable --expr "(* (log x) (log y))"
```

When an expression starts with `-`, pass it after `--` or wrap it:
```bash
cargo run -p mpl-simplify -- normalize --expr -- -7/3
cargo run -p mpl-simplify -- normalize --expr "(- 7/3)"
```

## Examples
```bash
# Division desugars into negative powers.
cargo run -p mpl-simplify -- normalize --expr "(/ x y z)"
# -> (* (^ y -1) (^ z -1) x)

# Powers fold and combine.
cargo run -p mpl-simplify -- normalize --expr "(^ (^ x 2) 3)"
# -> (^ x 6)

# Multiplication by zero collapses.
cargo run -p mpl-simplify -- normalize --expr "(* x 0 y)"
# -> 0

# Symbol generation (deterministic ordering).
cargo run -p mpl-simplify -- symbol --expr "(li2 x)"

# Integrability check (weight=2).
cargo run -p mpl-simplify -- check-integrable --expr "(* (log x) (log y))"
```

## Testing
```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
cargo bench
```

Regression cases live in `tests/regression_normalize.rs`.

## Known limitations
- Log/Li2 are only used as symbol carriers; no functional identities or branch
  handling are implemented.
- Symbol support is minimal: symbol(log), symbol(li2), symbol(log*log) only.
- Integrability checks are only implemented for weight=2 symbols.
- Letters in symbols must be algebraic (Rational/Var/Add/Mul/Pow/Neg).
- Negation is not distributed over `Add` and is only partially normalized.
