//! `compare <pid> [dll] [seconds]`: injects `bwrapi.dll` into a running
//! game, then reads the same sources from outside through
//! ReadProcessMemory and compares each frame with the digest the in-game
//! frame callback published. Opening the frame ring is what asks the
//! in-game DLL to read and publish per frame; closed, it reads nothing.

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::process::ExitCode;
    use std::time::{Duration, Instant};

    use bwrapi_engine::{frame_flag, Engine, Error, Table};
    use bwrapi_game::{
        players_digest, units_digest, GameFrames, GameStatus, FRAME_RING_CAPACITY, SOURCES,
    };

    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(pid) = args.first().and_then(|value| value.parse::<u32>().ok()) else {
        eprintln!("usage: compare <pid> [dll] [seconds]");
        return ExitCode::FAILURE;
    };
    let dll = args.get(1).map(PathBuf::from).unwrap_or_else(default_dll);
    let seconds: u64 = args.get(2).and_then(|value| value.parse().ok()).unwrap_or(10);

    if let Err(error) = bwrapi_game::inject(pid, &dll) {
        eprintln!("inject {}: {error}", dll.display());
        if let Some(status) = GameStatus::open(pid) {
            eprintln!("{status:?}");
        }
        return ExitCode::FAILURE;
    }
    let Some(ring) = GameFrames::open(pid) else {
        eprintln!("the game did not publish a frame ring");
        return ExitCode::FAILURE;
    };
    let mut engine = match Engine::open(pid) {
        Ok(engine) => engine,
        Err(error) => {
            eprintln!("open game {pid}: {error}");
            return ExitCode::FAILURE;
        }
    };
    engine.subscribe(SOURCES);

    /// One out-of-game read waiting for the in-game slot of its frame; the
    /// game thread may publish that slot only after this read.
    #[derive(Clone, Copy)]
    struct Read {
        game_frame: u32,
        unit_rows: u32,
        units_digest: u64,
        players_digest: u64,
        torn: bool,
    }

    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut pending: VecDeque<Read> = VecDeque::new();
    let mut out_times: Vec<u64> = Vec::new();
    let mut in_times: Vec<u64> = Vec::new();
    let mut compared = 0_usize;
    let mut matched = 0_usize;
    let mut mismatched = 0_usize;
    let mut torn_mismatched = 0_usize;
    let mut missed = 0_usize;
    let mut torn_reads = 0_usize;
    let mut mismatches: Vec<String> = Vec::new();
    let mut progress = Instant::now();

    while Instant::now() < deadline {
        match engine.step(Duration::from_millis(200)) {
            Ok(()) => {}
            Err(Error::Timeout) => continue,
            Err(Error::NotInGame) => {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            Err(error) => {
                eprintln!("read game {pid}: {error}");
                break;
            }
        }
        let start = Instant::now();
        let frame = match engine.acquire() {
            Ok(frame) => frame,
            Err(Error::NotInGame) => {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            Err(error) => {
                eprintln!("acquire: {error}");
                break;
            }
        };
        out_times.push(start.elapsed().as_micros() as u64);
        let torn = frame.flags & frame_flag::TORN != 0;
        torn_reads += torn as usize;
        if frame.game_frame != u32::MAX {
            pending.push_back(Read {
                game_frame: frame.game_frame,
                unit_rows: frame.units.rows() as u32,
                units_digest: units_digest(&frame.units),
                players_digest: players_digest(&frame.players),
                torn,
            });
        }
        // The in-game slot of a frame can appear after this side read it, so
        // pending reads resolve in any order; one whose frame left the ring
        // unpublished never will.
        let mut index = 0;
        while index < pending.len() {
            let read = pending[index];
            if let Some(slot) = ring.slot(read.game_frame) {
                pending.remove(index);
                compared += 1;
                in_times.push(u64::from(slot.acquire_us));
                if slot.units_digest == read.units_digest
                    && slot.players_digest == read.players_digest
                    && slot.unit_rows == read.unit_rows
                {
                    matched += 1;
                } else if read.torn {
                    torn_mismatched += 1;
                } else {
                    mismatched += 1;
                    if mismatches.len() < 5 {
                        mismatches.push(format!(
                            "frame {}: in rows {} digest {:x}, out rows {} digest {:x}",
                            read.game_frame, slot.unit_rows, slot.units_digest, read.unit_rows, read.units_digest
                        ));
                    }
                }
                continue;
            }
            if frame.game_frame != u32::MAX
                && frame.game_frame.wrapping_sub(read.game_frame) >= FRAME_RING_CAPACITY as u32
            {
                pending.remove(index);
                missed += 1;
                continue;
            }
            index += 1;
        }
        if progress.elapsed() >= Duration::from_secs(1) {
            println!(
                "compared {compared} matched {matched} mismatched {mismatched} torn_mismatched {torn_mismatched} missed {missed} pending {}",
                pending.len()
            );
            progress = Instant::now();
        }
    }

    println!(
        "frames compared={compared} matched={matched} mismatched={mismatched} torn_mismatched={torn_mismatched} missed={missed} torn_reads={torn_reads} unresolved={}",
        pending.len()
    );
    for mismatch in &mismatches {
        println!("mismatch {mismatch}");
    }
    println!("in-game first acquire {} us", ring.first_acquire_us());
    println!("{}", summary("in-game acquire", in_times));
    println!("{}", summary("out-of-game acquire", out_times));
    ExitCode::SUCCESS
}

/// `p50`/`p99`/`max` in microseconds, nearest-rank percentiles.
#[cfg(windows)]
fn summary(label: &str, mut times: Vec<u64>) -> String {
    if times.is_empty() {
        return format!("{label}: no samples");
    }
    times.sort_unstable();
    let at = |percent: u64| times[(times.len() as u64 * percent + 99) as usize / 100 - 1];
    format!("{}: p50={} p99={} max={} us", label, at(50), at(99), times[times.len() - 1])
}

/// `bwrapi.dll` next to this executable, or in the target directory above
/// `examples/`.
#[cfg(windows)]
fn default_dll() -> std::path::PathBuf {
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
    eprintln!("compare runs on Windows only");
}
