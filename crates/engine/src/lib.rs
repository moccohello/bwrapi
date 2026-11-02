//! Reads and decodes SCR game memory from outside the game process into
//! column tables of the subscribed sources.

mod columns;
pub mod command;
pub mod decode;
mod engine;
mod identity;
pub mod publish;
pub mod recorder;
mod read;
pub mod tables;

pub use columns::{Cell, CellKind, ColumnInfo, Columns, Table, Text, Text256, Text32};
pub use engine::{frame_flag, CommandBasis, Engine, Error, Frame, ResourceTotals, RESOURCE_SLOTS};
pub use identity::{unit_id, unit_slot};

/// In-memory game process; shared with tests of crates built on the engine.
#[cfg(any(test, feature = "test-support"))]
pub mod fake;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod publish_tests;
#[cfg(test)]
mod step_tests;
#[cfg(test)]
mod read_tests;
#[cfg(test)]
mod terrain_tests;
