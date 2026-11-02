//! The wait build of the frame module: same code under a file name that
//! turns the in-callback wait on (see `frame_module.rs`).
#![cfg(windows)]

#[path = "frame_module.rs"]
mod frame_module;
