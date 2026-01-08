# Symbol Space Engine (mpl-symbol)

This document describes the general weight=n symbol space engine in `crates/symbol`.

## Overview

The space engine constructs a basis for the integrable subspace of the full word
space at a given weight. It is deterministic by construction: word enumeration
is lexicographic, sampling is fixed, and all sparse rows are keyed by ordered
indices.

Key APIs (in `crate::space`):

- `Alphabet`, `WordConstraints`, and `WordAcceptor`: define the letter set and
  allowed words (constraints can be adapted to acceptors).
- `build_integrable_basis` and `build_integrable_basis_with_acceptor`:
  enumerate words, build constraints, and return a `Basis` whose vectors span
  the integrable subspace.
- `count_words_with_acceptor`: deterministic DP word counts with budgets.
- `check_integrable_n`: verify integrability for any weight.
- `reduce_to_basis`: express a symbol in the integrable basis (residual if not).
- `BasisStats`: standardized diagnostics for basis construction.

## Word Enumeration and Constraints

- Words are sequences of letter IDs of length `w`.
- Enumeration is lexicographic in ID order, left-to-right.
- `WordConstraints` can restrict the first letter and/or allowed adjacent pairs.
- `WordAcceptor` provides a composable DFA-style filter over words; adapters
  preserve the M1 constraints.
- `KGramAcceptor` (k=3) enforces allowed/forbidden triplets; an empty allow-list
  rejects all triplets once the context length reaches 2 (the experiments spec
  rejects empty allow-lists for safety).
- `GenealogicalAcceptor` enforces "after seeing X, forbid Y later" rules.
  The experiments spec defaults to channel-level tracking and uses a fixed-size
  bitset state to keep determinism and avoid state explosion.
- Experiments TOML can attach per-letter `channel` metadata (required when
  `seen = "channel"`) and optional constraint budgets via `[constraints.budget]`.

## Integrability Constraints

For each weight `w >= 2` and each adjacent position `k`:

1. Group terms by CONTEXT (the word with positions `k` and `k+1` removed).
2. For each context and variable pair `(vi, vj)`, require:
   sum(coeff * wedge(dlog(l_k), dlog(l_{k+1}))) == 0

The wedge is evaluated on deterministic rational samples. Singular samples are
skipped. If fewer than two valid samples exist for a constraint, the engine
returns `SymbolError::InsufficientSamples`.

## Linear Algebra Approach

Constraint rows are inserted into a streaming REF (dictionary form) system:

- Each pivot row has the smallest column index as its pivot (normalized to 1).
- New rows are reduced only against existing pivots with the same smallest
  column index.
- No global RREF cleanup is performed (avoids quadratic overhead).

After streaming insertion, the nullspace is recovered by back-substitution:

- Free columns are the non-pivot columns in ascending order.
- For each free column `f`, set `x[f]=1`, others 0, then solve pivot variables
  in descending pivot order.

This yields a basis where each vector has a single 1 at its free column,
allowing exact coefficient recovery in `reduce_to_basis`.

## BasisStats

`BasisStats` collects deterministic diagnostics such as:

- column count, rank, dimension
- rows attempted/inserted, row sparsity stats
- `samples_used` (valid sampled constraint-rows) and `envs_total`
- skip reasons (`rows_skipped_singular`, `constraints_insufficient_samples`)

Use `BasisStats::one_line()` for stable, compact output in stress tests.

## Determinism Rules

- Never rely on `HashMap` iteration order.
- Keep pivot selection deterministic (smallest column index).
- Maintain stable word ordering and stable sampling order.
