use std::sync::Arc;
use std::time::Duration;

use bwrapi_layout::Layout;

use crate::decode::bwgame::BWGAME_FRAME_COUNT_OFFSET;
use crate::fake::{in_game, Shared, ROOT};
use crate::{Engine, Error};

#[test]
fn step_returns_once_the_game_frame_moves_past_the_acquired_one() {
    let memory = in_game(4);
    memory.patch(ROOT + BWGAME_FRAME_COUNT_OFFSET, &10u32.to_le_bytes());
    let mut engine = Engine::with_memory(Box::new(Shared(Arc::clone(&memory)))).unwrap();
    assert_eq!(engine.acquire().unwrap().game_frame, 10);
    assert_eq!(engine.step(Duration::from_millis(5)), Err(Error::Timeout));

    let writer = Arc::clone(&memory);
    let advance = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        writer.patch(ROOT + BWGAME_FRAME_COUNT_OFFSET, &11u32.to_le_bytes());
    });
    assert_eq!(engine.step(Duration::from_secs(5)), Ok(()));
    advance.join().unwrap();
    assert_eq!(engine.acquire().unwrap().game_frame, 11);

    memory.patch(memory.module.base_address + Layout::X64.scalars.session_is_in_game_offset, &0u32.to_le_bytes());
    assert_eq!(engine.step(Duration::from_millis(5)), Err(Error::NotInGame));
}

#[test]
fn command_basis_reports_the_last_acquire_and_finds_the_pool_without_unit_sources() {
    use crate::fake::POOL;
    use crate::tables::source;
    use crate::CommandBasis;

    let memory = in_game(4);
    memory.patch(ROOT + BWGAME_FRAME_COUNT_OFFSET, &10u32.to_le_bytes());
    for sources in [0, source::UNITS] {
        let mut engine = Engine::with_memory(Box::new(Shared(Arc::clone(&memory)))).unwrap();
        engine.subscribe(sources);
        assert_eq!(engine.command_basis(), None);
        engine.acquire().unwrap();
        assert_eq!(
            engine.command_basis(),
            Some(CommandBasis { root: ROOT as u64, pool_base: POOL as u64, local_player: u8::MAX, game_frame: 10, match_id: 1 })
        );
        // The basis stays the last acquire's even when the game moves on.
        memory.patch(ROOT + BWGAME_FRAME_COUNT_OFFSET, &11u32.to_le_bytes());
        assert_eq!(engine.command_basis().unwrap().game_frame, 10);
        memory.patch(ROOT + BWGAME_FRAME_COUNT_OFFSET, &10u32.to_le_bytes());
    }

    let mut engine = Engine::with_memory(Box::new(Shared(Arc::clone(&memory)))).unwrap();
    engine.acquire().unwrap();
    memory.patch(memory.module.base_address + Layout::X64.scalars.session_is_in_game_offset, &0u32.to_le_bytes());
    assert_eq!(engine.acquire().err(), Some(Error::NotInGame));
    assert_eq!(engine.command_basis(), None);
}

#[test]
fn command_basis_is_none_when_the_pool_pointer_is_missing() {
    let memory = in_game(4);
    memory.patch(
        memory.module.base_address + Layout::X64.cunit.array_pointer_rva,
        &0u64.to_le_bytes(),
    );
    let mut engine = Engine::with_memory(Box::new(Shared(Arc::clone(&memory)))).unwrap();
    engine.acquire().unwrap();
    assert_eq!(engine.command_basis(), None);
}
