mod calculus;
mod error;
mod eval;
mod hopf;
mod integrability;
mod integrability_utils;
mod projection;
mod rules;
pub mod space;
mod tensor;
mod word;

pub use error::{ConstraintBudgetKind, EvalError, SymbolError};
pub use hopf::Coproduct;
pub use integrability::check_integrable;
pub use projection::{apply_suffix_projection, apply_suffix_projection_to_basis};
pub use rules::{symbol, symbol_with_fuel};
pub use tensor::{Coeff, ShuffleFuel, Symbol};
pub use word::Word;
