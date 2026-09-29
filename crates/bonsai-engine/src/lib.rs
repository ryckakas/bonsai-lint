//! The language registry, configuration and domain discovery, baselines and scan driver behind
//! the multi-language `bonsai-lint` cognitive complexity linter.
//!
//! [`config::discover`] reads the [`Workspace`] a path belongs to, [`Scanner::scan`] scores the
//! files under it, and a [`Baseline`] tells new findings from accepted ones.

mod pool;

pub mod baseline;
pub mod config;
pub mod finding;
pub mod registry;
pub mod report;
pub mod scan;

pub use baseline::Baseline;
pub use config::{Domain, Workspace};
pub use finding::Located;
pub use pool::STACK_SIZE;
pub use scan::{ScanOutcome, ScanStats, Scanner};
