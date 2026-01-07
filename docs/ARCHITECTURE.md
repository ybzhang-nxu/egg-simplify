# Architecture Overview

This workspace is a deterministic Rust simplifier with a strict dependency DAG.

## Crate Responsibilities

- `mpl-ir`: expression AST, parsing, normalization, canonical printing.
- `mpl-symbol`: symbol tensor, symbolization rules, integrability checks, space engine.
- `mpl-rewrite`: egg language, rewrite rules, lowering/lifting, extractor.
- `mpl-rewrite-symbol`: symbol-aware rewrite pipeline, fingerprint cache, stable extractor tie.
- `mpl-verify`: exact rational evaluation and sample-based equivalence.
- `mpl-simplify`: CLI entry point.

## Dependency DAG (no cycles)

```
mpl-ir  <- mpl-symbol
mpl-ir  <- mpl-rewrite
mpl-ir  <- mpl-verify
mpl-rewrite-symbol <- {mpl-ir, mpl-rewrite, mpl-symbol}
mpl-simplify -> {mpl-ir, mpl-rewrite, mpl-symbol, mpl-rewrite-symbol}
```

## Determinism Guarantees

- Canonical normalization is idempotent and deterministic.
- Symbol terms are stored in ordered maps and printed in canonical order.
- Space engine word enumeration is lexicographic; pivots are chosen by smallest
  column index; sampling uses a fixed table.
- Symbol-aware extraction uses a deterministic structural hash to break ties.

See `docs/canonical_form.md` and `docs/space_engine.md` for detailed specs.
