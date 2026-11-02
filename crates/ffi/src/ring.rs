//! The shared command ring of the in-game DLL: the frame hook of a running,
//! injected game executes what a session queues there, so submission never
//! blocks on the game.

use bwrapi_game::{GameCommands, GameStatus, hook_state};

use crate::command::{BwrResult, RESULT_PENDING, RESULT_UNAVAILABLE};

/// The command ring of game `pid`, when its in-game DLL runs the frame hook.
pub(crate) fn open(pid: u32) -> Option<GameCommands> {
    GameStatus::open(pid)
        .is_some_and(|status| status.hook_state == hook_state::INSTALLED)
        .then(|| GameCommands::open(pid))
        .flatten()
}

/// Reads one ring result: no entry yet is pending, not unknown — the game
/// thread publishes it at the next frame boundary. Id 0 marks a submit that
/// failed before the ring took the command.
pub(crate) fn poll(ring: &GameCommands, id: u64) -> BwrResult {
    if id == 0 {
        return BwrResult::new(0, RESULT_UNAVAILABLE);
    }
    match ring.result(id) {
        Some((state, frame)) => BwrResult { id, state, frame_hint: frame, actions: 0, reserved: 0 },
        None => BwrResult::new(id, RESULT_PENDING),
    }
}
