//! Voxa's shared core.
//!
//! The dictation loop from `docs/rebuild/01-flow-audit.md`, as a pure state machine that every
//! shell (desktop, Android) drives the same way:
//!
//! ```text
//! Idle --hotkey--> [readiness check] --ok--> Recording --stop--> Transcribing --text--> Delivering --> Delivered
//!                         |                                           |                       |
//!                         v                                           v                       v
//!                    Blocked(why)                              NeedsYou(problem, job) <-------+
//! ```
//!
//! Shells feed [`flow::Event`]s in and carry out the returned [`flow::Effect`]s (start the mic, call
//! ChatGPT, paste, show the strip). The core never does I/O itself, so every rule is unit-tested.

pub mod copy;
pub mod doodle;
pub mod error;
pub mod flow;
pub mod greeting;
pub mod job;
pub mod readiness;
pub mod splash;
pub mod svgpath;
pub mod text;
pub mod trim;

pub use error::TranscribeError;
pub use flow::{Effect, Event, Flow, Problem, State};
pub use job::{Job, JobId, JobState, Target};
pub use readiness::{Blocker, Mic, Network, Readiness, Session};
