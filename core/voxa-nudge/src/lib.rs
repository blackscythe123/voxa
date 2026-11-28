//! Voxa's "come back" reminder engine.
//!
//! Decides WHEN to send a re-engagement reminder ("You haven't dictated in a few days") and WHICH
//! wording, on the device, in microseconds:
//!
//! - [`rules`]: hard limits checked in code before the model runs (at most one every 3 days, never
//!   21:00 to 09:00, never within 48 hours of a dictation, stop after 2 ignored until the user
//!   dictates again, an off switch). The model can never override them.
//! - [`net`]: a tiny network (195 possible connections, 13 after pruning) giving P(send
//!   now) and a softmax over four wordings, trained by neuroevolution ([`evolve`]) in a user
//!   simulator ([`sim`]).
//! - [`bandit`]: Thompson sampling over styles and send-time slots, with the network's output as
//!   the prior, so each phone adapts to its user.
//! - [`engine`]: the three together, with the state saved as JSON ([`state`]).
//!
//! No ML framework: plain Rust, serde for the JSON.

pub mod bandit;
pub mod evolve;
pub mod features;
pub mod messages;
pub mod net;
pub mod rng;
pub mod rules;
pub mod state;

pub use messages::Style;
pub use rules::Block;
pub use state::{NudgeState, Outcome};
