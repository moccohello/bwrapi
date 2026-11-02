//! `command <pid> [dll] <kind>`: injects `bwrapi.dll` into a running game,
//! reads it from outside with the engine, and sends one command straight
//! through the shared command ring — no FFI. It then reports the command's
//! result and what the acting unit's row says a moment later.
//!
//! Kinds: move, attack, train, gather, build, stop, research, upgrade. Each
//! picks its own actors from the local player's units: a worker for move,
//! gather, build and stop; the town hall for train; the race's research or
//! upgrade building when one exists. Zerg builds no supply building, so its
//! `build` only reports that.

#[cfg(windows)]
use std::path::PathBuf;
#[cfg(windows)]
use std::time::{Duration, Instant};

#[cfg(windows)]
use bwrapi_engine::command::{command_kind, UnitCommand};
#[cfg(windows)]
use bwrapi_engine::tables::source;
#[cfg(windows)]
use bwrapi_engine::tables::Units;
#[cfg(windows)]
use bwrapi_engine::{Engine, Table};
#[cfg(windows)]
use bwrapi_game::GameCommands;

/// BWAPI unit type ids the pickers need.
#[cfg(windows)]
mod types {
    pub const WORKERS: [u16; 3] = [7, 41, 64];
    pub const TOWN_HALLS: [u16; 3] = [106, 131, 154];
    pub const MINERALS: [u16; 3] = [176, 177, 178];
    /// Worker type to its race's supply building.
    pub const SUPPLY: [(u16, u16); 2] = [(7, 109), (64, 156)];
    /// Research building to the tech it offers.
    pub const RESEARCH: [(u16, u32); 3] = [(112, 0), (135, 32), (165, 19)];
    /// Upgrade building to the upgrade it offers.
    pub const UPGRADE: [(u16, u32); 3] = [(122, 0), (142, 10), (166, 13)];
}

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: command <pid> [dll] <move|attack|train|gather|build|stop|research|upgrade>";
    let Some(pid) = args.first().and_then(|value| value.parse::<u32>().ok()) else {
        eprintln!("{usage}");
        return std::process::ExitCode::FAILURE;
    };
    let Some(kind) = args.last().map(String::as_str).filter(|kind| kind != &args[0]) else {
        eprintln!("{usage}");
        return std::process::ExitCode::FAILURE;
    };
    let kind = kind.to_ascii_lowercase();
    let dll = (args.len() == 3).then(|| PathBuf::from(&args[1]));

    if let Err(error) = bwrapi_game::inject(pid, &dll.unwrap_or_else(default_dll)) {
        eprintln!("inject: {error}");
        return std::process::ExitCode::FAILURE;
    }
    let Some(ring) = GameCommands::open(pid) else {
        eprintln!("the game publishes no command ring");
        return std::process::ExitCode::FAILURE;
    };
    let Some(mut engine) = open_in_match(pid) else {
        eprintln!("no running match in process {pid}");
        return std::process::ExitCode::FAILURE;
    };
    let frame = engine.acquire().expect("the match was open a moment ago");
    let basis = engine.command_basis().expect("the match was open a moment ago");
    let local = frame.local_player;
    if local == u8::MAX {
        eprintln!("this session observes; it commands nobody");
        return std::process::ExitCode::FAILURE;
    }
    println!("frame {} local player {} units {}", frame.game_frame, local, frame.units.rows());

    let Some((command, watched)) =
        build_kind(&frame.units, local, &kind, frame.map_width)
    else {
        eprintln!("no unit for {kind} in this match");
        return std::process::ExitCode::FAILURE;
    };
    println!(
        "{kind}: {} unit(s), target {:#x}, at ({}, {})",
        command.units.len(),
        command.target,
        command.x,
        command.y
    );
    let actor = command.units[0];
    print_row(&frame.units, actor, "actor before ");
    if watched != 0 && watched != actor {
        print_row(&frame.units, watched, "target before ");
    }
    let Ok(id) = ring.submit(&basis, &command) else {
        eprintln!("submit refused");
        return std::process::ExitCode::FAILURE;
    };
    let (state, result_frame) = wait_result(&ring, id);
    println!("result {state} ({}) at game frame {result_frame}", state_name(state));

    // A moment of game time, then read what the unit rows say.
    std::thread::sleep(Duration::from_millis(1200));
    if let Ok(frame) = engine.acquire() {
        print_row(&frame.units, actor, "actor after  ");
        if state == 1 {
            report_effect(&frame.units, actor, &kind);
        }
    }
    std::process::ExitCode::SUCCESS
}

/// Opens the engine and waits for a frame inside a match.
#[cfg(windows)]
fn open_in_match(pid: u32) -> Option<Engine> {
    let mut engine = Engine::open(pid).ok()?;
    engine.subscribe(source::UNITS);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let frame = engine.acquire().ok()?;
        if frame.game_frame != u32::MAX {
            return Some(engine);
        }
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Picks the acting units and fills the command for `kind`; returns the id
/// whose row shows the effect (0: the effect is on the actors themselves).
#[cfg(windows)]
fn build_kind(
    units: &Units,
    local: u8,
    kind: &str,
    map_width: u16,
) -> Option<(UnitCommand, u64)> {
    let rows = units.rows();
    let own = |row: usize| units.owner[row] == local;
    let worker = (0..rows).find(|row| own(*row) && types::WORKERS.contains(&units.unit_type[*row]));
    let town_hall =
        (0..rows).find(|row| own(*row) && types::TOWN_HALLS.contains(&units.unit_type[*row]));
    let nearest = |from: usize, predicate: &dyn Fn(usize) -> bool| -> Option<usize> {
        let mut best: Option<(i64, usize)> = None;
        for row in 0..rows {
            if !predicate(row) {
                continue;
            }
            let dx = i64::from(units.x[row] - units.x[from]);
            let dy = i64::from(units.y[row] - units.y[from]);
            let distance = dx * dx + dy * dy;
            if best.is_none_or(|(best_distance, _)| distance < best_distance) {
                best = Some((distance, row));
            }
        }
        best.map(|(_, row)| row)
    };
    let base = |ids: &[u64]| UnitCommand { units: ids.to_vec(), ..UnitCommand::default() };

    match kind {
        "move" => {
            let row = worker?;
            let x = units.x[row] as i32 + 8 * 32;
            let x = if map_width == 0 { x } else { x.min(i32::from(map_width) * 32 - 1) };
            let y = units.y[row] as i32;
            Some((UnitCommand { kind: command_kind::MOVE, x, y, ..base(&[units.id[row]]) }, 0))
        }
        "attack" => {
            let row = worker.or(town_hall)?;
            match nearest(row, &|other: usize| {
                let owner = units.owner[other];
                owner != local && owner < 8 && !types::MINERALS.contains(&units.unit_type[other])
            }) {
                Some(enemy) => Some((
                    UnitCommand {
                        kind: command_kind::ATTACK_UNIT,
                        target: units.id[enemy],
                        ..base(&[units.id[row]])
                    },
                    units.id[enemy],
                )),
                // Nobody to attack: attack the ground nearby instead.
                None => Some((
                    UnitCommand {
                        kind: command_kind::ATTACK_POSITION,
                        x: units.x[row] as i32 + 10 * 32,
                        y: units.y[row] as i32,
                        ..base(&[units.id[row]])
                    },
                    0,
                )),
            }
        }
        "train" => {
            let hall = town_hall?;
            // Zerg trains from a larva, not from the hatchery.
            let (source, worker) = match units.unit_type[hall] {
                106 => (hall, 7),
                131 => ((0..rows).find(|row| own(*row) && units.unit_type[*row] == 35)?, 41),
                _ => (hall, 64),
            };
            Some((
                UnitCommand { kind: command_kind::TRAIN, type_id: worker as u32, ..base(&[units.id[source]]) },
                units.id[source],
            ))
        }
        "gather" => {
            let row = worker?;
            let mineral =
                nearest(row, &|other: usize| types::MINERALS.contains(&units.unit_type[other]))?;
            Some((
                UnitCommand { kind: command_kind::GATHER, target: units.id[mineral], ..base(&[units.id[row]]) },
                units.id[mineral],
            ))
        }
        "build" => {
            let row = worker?;
            let worker_type = units.unit_type[row];
            let Some(&(_, supply)) =
                types::SUPPLY.iter().find(|(race_worker, _)| *race_worker == worker_type)
            else {
                eprintln!("zerg supply is a morph, not a worker build; try gather or move");
                return None;
            };
            let tile_x = units.x[row] as i32 / 32 + 3;
            let tile_y = units.y[row] as i32 / 32 + 3;
            Some((
                UnitCommand {
                    kind: command_kind::BUILD,
                    type_id: supply as u32,
                    x: tile_x,
                    y: tile_y,
                    ..base(&[units.id[row]])
                },
                units.id[row],
            ))
        }
        "stop" => {
            worker.map(|row| (UnitCommand { kind: command_kind::STOP, ..base(&[units.id[row]]) }, 0))
        }
        "research" => {
            let building = (0..rows).find(|row| {
                own(*row) && types::RESEARCH.iter().any(|(building_type, _)| *building_type == units.unit_type[*row])
            })?;
            let tech =
                types::RESEARCH.iter().find(|(building_type, _)| *building_type == units.unit_type[building])?.1;
            Some((
                UnitCommand { kind: command_kind::RESEARCH, type_id: tech, ..base(&[units.id[building]]) },
                units.id[building],
            ))
        }
        "upgrade" => {
            let building = (0..rows).find(|row| {
                own(*row) && types::UPGRADE.iter().any(|(building_type, _)| *building_type == units.unit_type[*row])
            })?;
            let upgrade =
                types::UPGRADE.iter().find(|(building_type, _)| *building_type == units.unit_type[building])?.1;
            Some((
                UnitCommand { kind: command_kind::UPGRADE, type_id: upgrade, ..base(&[units.id[building]]) },
                units.id[building],
            ))
        }
        _ => {
            eprintln!("unknown kind {kind}");
            None
        }
    }
}

#[cfg(windows)]
fn print_row(units: &Units, id: u64, when: &str) {
    let Some(row) = (0..units.rows()).find(|row| units.id[*row] == id) else {
        println!("{when}unit {id:#x}: gone");
        return;
    };
    println!(
        "{when}unit {id:#x} type {} order {} at ({}, {})",
        units.unit_type[row], units.order[row], units.x[row], units.y[row]
    );
    println!(
        "{when}  order target ({}, {}) target unit {:#x}",
        units.order_target_x[row], units.order_target_y[row], units.order_target[row]
    );
}

#[cfg(windows)]
fn report_effect(units: &Units, id: u64, kind: &str) {
    let Some(row) = (0..units.rows()).find(|row| units.id[*row] == id) else { return };
    match kind {
        "train" => println!(
            "train queue head {} (type 0 is none), {} frames left",
            units.train_queue_0[row], units.remaining_train_frames[row]
        ),
        "research" => println!(
            "researching tech {} with {} frames left",
            units.tech[row], units.remaining_research_frames[row]
        ),
        "upgrade" => println!(
            "upgrading {} with {} frames left",
            units.upgrade[row], units.remaining_upgrade_frames[row]
        ),
        "build" => println!(
            "building type {} with {} frames left",
            units.build_type[row], units.remaining_build_frames[row]
        ),
        _ => println!("order target ({}, {})", units.order_target_x[row], units.order_target_y[row]),
    }
}

#[cfg(windows)]
fn wait_result(ring: &GameCommands, id: u64) -> (u32, u32) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(outcome) = ring.result(id) {
            return outcome;
        }
        if Instant::now() > deadline {
            return (0, 0);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(windows)]
fn state_name(state: u32) -> &'static str {
    match state {
        1 => "executed",
        2 => "format error",
        3 => "not in a match",
        4 => "unit gone",
        _ => "pending",
    }
}

/// `bwrapi.dll` next to this executable, or in the target directory above
/// `examples/`.
#[cfg(windows)]
fn default_dll() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let directory = exe.parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    let beside = directory.join("bwrapi.dll");
    if beside.is_file() {
        return beside;
    }
    directory.parent().map(|target| target.join("bwrapi.dll")).unwrap_or(beside)
}

#[cfg(not(windows))]
fn main() {
    eprintln!("command runs on Windows only");
}
