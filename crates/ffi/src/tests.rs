use std::sync::Arc;

use bwrapi_engine::tables::{source, UnitRow, ViewportRow};
use bwrapi_engine::Frame;

use super::*;

#[test]
fn open_reports_invalid_arguments() {
    unsafe {
        assert_eq!(bwr_open(1, 0, std::ptr::null_mut()), BwrStatus::Invalid);
        let mut session = std::ptr::null_mut();
        assert_eq!(bwr_open(u32::MAX, 0, &mut session), BwrStatus::Gone);
        assert!(session.is_null());
        assert!(!std::ffi::CStr::from_ptr(bwr_last_error(std::ptr::null())).to_bytes().is_empty());
        let mut frame = std::ptr::null();
        assert_eq!(bwr_acquire(std::ptr::null_mut(), &mut frame), BwrStatus::Invalid);
        assert_eq!(bwr_subscribe(std::ptr::null_mut(), source::UNITS), BwrStatus::Invalid);
    }
    assert_eq!(bwr_abi_version(), 1 << 16);
}

#[test]
fn open_self_opens_the_calling_process_or_reports_why_not() {
    unsafe {
        assert_eq!(bwr_open_self(std::ptr::null_mut()), BwrStatus::Invalid);
        let mut session = std::ptr::null_mut();
        // The test process is not a supported game build; off Windows the
        // platform read is unsupported.
        let expected = if cfg!(windows) { BwrStatus::UnsupportedBuild } else { BwrStatus::Gone };
        assert_eq!(bwr_open_self(&mut session), expected);
        assert_eq!(session.is_null(), expected != BwrStatus::Ok);
        bwr_close(session);
    }
}

extern "C" fn recorded_callback(_user: *mut std::ffi::c_void) {}

#[test]
fn the_frame_callback_belongs_to_self_sessions() {
    use bwrapi_engine::fake::{in_game, Shared};
    use bwrapi_engine::Engine;

    unsafe {
        assert_eq!(
            bwr_set_frame_callback(std::ptr::null_mut(), None, std::ptr::null_mut()),
            BwrStatus::Invalid
        );
        // A session of another process is not the game itself, whatever it
        // acquired: the game thread would never run its callback.
        let mut engine = Engine::with_memory(Box::new(Shared(in_game(8)))).unwrap();
        engine.acquire().unwrap();
        let mut session = BwrSession::new(engine);
        assert_eq!(
            bwr_set_frame_callback(&mut session, None, std::ptr::null_mut()),
            BwrStatus::Invalid
        );
        assert_eq!(
            bwr_set_frame_callback(&mut session, Some(recorded_callback), std::ptr::null_mut()),
            BwrStatus::Invalid
        );
    }
}

#[cfg(windows)]
#[test]
fn a_self_session_without_the_frame_hook_refuses_the_callback() {
    use bwrapi_engine::fake::{in_game, Shared};
    use bwrapi_engine::Engine;

    unsafe {
        let mut owned = BwrSession::new(Engine::with_memory(Box::new(Shared(in_game(8)))).unwrap());
        owned.is_self = true;
        let session: *mut BwrSession = &mut owned;
        // The test process runs no frame hook, so nothing would invoke the
        // callback.
        assert_eq!(
            bwr_set_frame_callback(session, Some(recorded_callback), std::ptr::null_mut()),
            BwrStatus::NotInGame
        );
        assert!(!std::ffi::CStr::from_ptr(bwr_last_error(session)).to_bytes().is_empty());
    }
}

#[test]
fn a_leased_frame_points_at_the_columns_of_its_sources_only() {
    let mut frame = Frame { sources: source::UNITS | source::VIEWPORT, game_frame: 77, ..Frame::default() };
    frame.units.push(UnitRow { id: 5, unit_type: 7, x: 100, ..UnitRow::UNKNOWN });
    frame.units.push(UnitRow { id: 9, unit_type: 37, x: 200, ..UnitRow::UNKNOWN });
    frame.viewport.push(ViewportRow { x: 640, y: 480, ..ViewportRow::UNKNOWN });
    let lease = Box::into_raw(lease(Arc::new(frame))).cast::<BwrFrame>();
    unsafe {
        let view = &*lease;
        assert_eq!(view.game_frame, 77);
        assert!(view.tables[0].is_null());
        // Units: rows, then one pointer per column in COLUMNS order (id, unit_type, owner, x, ...).
        let units = view.tables[1];
        assert_eq!(*units, 2);
        let ids = *units.add(1) as *const u64;
        let types = *units.add(2) as *const u16;
        let xs = *units.add(4) as *const i16;
        assert_eq!(std::slice::from_raw_parts(ids, 2), [5, 9]);
        assert_eq!(std::slice::from_raw_parts(types, 2), [7, 37]);
        assert_eq!(std::slice::from_raw_parts(xs, 2), [100, 200]);
        let viewport = view.tables[7];
        assert_eq!(*(*viewport.add(2) as *const i32), 480);
        bwr_release(std::ptr::null_mut(), lease);
    }
}

#[test]
fn the_header_lists_every_column() {
    let header = generate::c_header();
    assert!(header.contains("typedef struct BwrUnits {\n    size_t rows;\n    const uint64_t* id;"));
    assert!(header.contains("const BwrOrders* orders;"));
    assert!(generate::csharp().contains("public BwrUnits? Units => _units == null ? null : *_units;"));
    assert!(generate::python().contains("(\"units\", POINTER(Units)),"));
}

mod submit {
    use bwrapi_engine::fake::{in_game, Shared};
    use bwrapi_engine::Engine;

    use crate::command::{COMMAND_KINDS, GAME_COMMAND_KINDS, RESULT_STATES};
    use crate::*;

    /// A session on a game without the in-game DLL: it reads, but has no
    /// command ring.
    struct Rig(*mut BwrSession);

    impl Drop for Rig {
        fn drop(&mut self) {
            unsafe { bwr_close(self.0) }
        }
    }

    fn rig() -> Rig {
        let mut engine = Engine::with_memory(Box::new(Shared(in_game(8)))).unwrap();
        engine.acquire().unwrap();
        Rig(Box::into_raw(Box::new(BwrSession::new(engine))))
    }

    fn command(units: &[u64]) -> BwrCommand {
        BwrCommand {
            size: std::mem::size_of::<BwrCommand>() as u32,
            kind: 0,
            flags: 1,
            type_id: 0,
            slot: 0,
            x: 320,
            y: 64,
            target: 0,
            units: units.as_ptr(),
            unit_count: units.len() as u32,
            reserved: 0,
            text: std::ptr::null(),
        }
    }

    #[test]
    fn malformed_calls_are_invalid() {
        let rig = rig();
        let units = [1u64];
        let mut ids = [0u64; 2];
        let good = command(&units);
        let mut wrong_size = command(&units);
        wrong_size.size -= 1;
        let mut no_units = command(&units);
        no_units.unit_count = 0;
        let mut null_units = command(&units);
        null_units.units = std::ptr::null();
        unsafe {
            assert_eq!(bwr_submit(std::ptr::null_mut(), &good, 1, ids.as_mut_ptr()), BwrStatus::Invalid);
            assert_eq!(bwr_submit(rig.0, std::ptr::null(), 1, ids.as_mut_ptr()), BwrStatus::Invalid);
            assert_eq!(bwr_submit(rig.0, &good, 0, ids.as_mut_ptr()), BwrStatus::Invalid);
            assert_eq!(bwr_submit(rig.0, &good, 1, std::ptr::null_mut()), BwrStatus::Invalid);
            for bad in [wrong_size, no_units, null_units] {
                let batch = [command(&units), bad];
                assert_eq!(bwr_submit(rig.0, batch.as_ptr(), 2, ids.as_mut_ptr()), BwrStatus::Invalid);
            }
            let mut out = BwrResult::new(0, 0);
            assert_eq!(bwr_poll_result(rig.0, 1, std::ptr::null_mut()), BwrStatus::Invalid);
            assert_eq!(bwr_poll_result(std::ptr::null_mut(), 1, &mut out), BwrStatus::Invalid);
        }
        assert_eq!(ids, [0, 0]);
    }

    #[test]
    fn a_game_without_the_in_game_dll_refuses_commands() {
        let rig = rig();
        let units = [1u64];
        let good = command(&units);
        let mut ids = [0u64; 1];
        let mut out = BwrResult::new(0, 0);
        unsafe {
            assert_eq!(bwr_submit(rig.0, &good, 1, ids.as_mut_ptr()), BwrStatus::NoCommands);
            assert_eq!(bwr_poll_result(rig.0, 1, &mut out), BwrStatus::NoCommands);
        }
    }

    #[test]
    fn the_command_layout_and_numbering_match_the_header() {
        assert_eq!(std::mem::size_of::<BwrCommand>(), 64);
        assert_eq!(std::mem::size_of::<BwrResult>(), 24);
        assert_eq!(std::mem::offset_of!(BwrCommand, units), 40);
        assert_eq!(std::mem::offset_of!(BwrCommand, text), 56);
        for (index, (_, kind)) in COMMAND_KINDS.iter().enumerate() {
            assert_eq!(*kind, index as u32);
        }
        // Game kinds run on from 100 and are exactly the engine's game commands.
        for (index, (_, kind)) in GAME_COMMAND_KINDS.iter().enumerate() {
            assert_eq!(*kind, 100 + index as u32);
            assert!(bwrapi_engine::command::is_game_command(*kind));
        }
        assert!(!bwrapi_engine::command::is_game_command(100 + GAME_COMMAND_KINDS.len() as u32));
        assert_eq!(RESULT_STATES.map(|(_, value)| value), [0, 1, 2, 3, 4, 5, 6]);

        let header = generate::c_header();
        for line in [
            "#define BWR_COMMAND_MOVE 0u",
            "#define BWR_COMMAND_ATTACK_UNIT 2u",
            "#define BWR_COMMAND_PLACE_COP 43u",
            "#define BWR_COMMAND_SET_ALLIES 109u",
            "#define BWR_COMMAND_SET_MAP 113u",
            "#define BWR_COMMAND_FLAG_QUEUED 1u",
            "#define BWR_RESULT_UNKNOWN 6u",
            "    const uint64_t* units;",
            "BwrStatus bwr_submit(BwrSession* session, const BwrCommand* commands, uint32_t count, uint64_t* out_ids);",
            "BwrStatus bwr_poll_result(BwrSession* session, uint64_t id, BwrResult* out);",
            "BwrStatus bwr_set_frame_callback(BwrSession* session, void (*callback)(void* user), void* user);",
        ] {
            assert!(header.contains(line), "{line}");
        }
        let csharp = generate::csharp();
        assert!(csharp.contains("SmartTargetUnit = 1,") && csharp.contains("public ulong* Units;"));
        assert!(csharp.contains("Submit(nint session, BwrCommand* commands"));
        assert!(csharp.contains("SetFrameCallback(nint session, nint callback, nint user);"));
        let python = generate::python();
        assert!(python.contains("COMMAND_PLACE_COP = 43") && python.contains("\"bwr_poll_result\""));
        assert!(python.contains("\"bwr_set_frame_callback\": (c_uint32, [c_void_p, CFUNCTYPE(None, c_void_p), c_void_p]),"));
    }
}
