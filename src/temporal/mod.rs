pub mod propagate;
pub mod validate;

pub use propagate::{propagate_mask, PropagationConfig};
pub use validate::validate_propagation;
