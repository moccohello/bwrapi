//! Reads a running game without injection and reports read times:
//! `read_live <pid> [frames] [sources-hex]`.

#[cfg(windows)]
fn main() {
    use bwrapi_engine::{tables::source, Engine, Error};
    use std::time::{Duration, Instant};

    let mut args = std::env::args().skip(1);
    let pid: u32 = args.next().expect("pid").parse().expect("pid");
    let frames: usize = args.next().map_or(200, |value| value.parse().expect("frames"));
    let sources = args
        .next()
        .map_or(u64::MAX, |value| u64::from_str_radix(value.trim_start_matches("0x"), 16).expect("sources"));

    let mut engine = Engine::open(pid).expect("open");
    engine.subscribe(sources & ((source::REGION_NEIGHBORS << 1) - 1));
    let first = Instant::now();
    if engine.acquire().is_ok() {
        println!("first acquire (reads match tables) ms {:.3}", first.elapsed().as_secs_f64() * 1000.0);
    }
    let mut times = Vec::with_capacity(frames);
    let mut last = None;
    while times.len() < frames {
        let start = Instant::now();
        match engine.acquire() {
            Ok(frame) => {
                times.push(start.elapsed());
                last = Some(frame);
            }
            Err(Error::NotInGame) => {
                println!("not in game");
                return;
            }
            Err(error) => panic!("{error}"),
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let frame = last.expect("a frame");
    println!(
        "match {} frame {} flags {} local {} sources {:#x} players {} units {} counts {} upgrades {} techs {} bullets {} vision {} ({}x{}) selection {:?} orders {} viewport {:?}",
        frame.match_id,
        frame.game_frame,
        frame.flags,
        frame.local_player,
        frame.sources,
        frame.players.id.len(),
        frame.units.id.len(),
        frame.unit_counts.player.len(),
        frame.upgrades.player.len(),
        frame.techs.player.len(),
        frame.bullets.id.len(),
        frame.vision.visible.len(),
        frame.map_width,
        frame.map_height,
        frame.selection.unit,
        frame.orders.unit.len(),
        frame.viewport.row(0),
    );
    for index in 0..frame.players.id.len() {
        let p = frame.players.row(index).unwrap();
        println!("  player {} {:?} race {} type {} force {} color {} min {} gas {} supply z{}/{} t{}/{} p{}/{} start {},{} allies {:#x} gathered {} {}",
            p.id, p.name.as_str(), p.race, p.player_type, p.force, p.color, p.minerals, p.gas,
            p.supply_used_zerg, p.supply_total_zerg, p.supply_used_terran, p.supply_total_terran, p.supply_used_protoss, p.supply_total_protoss,
            p.start_x, p.start_y, p.allies, p.gathered_minerals, p.gathered_gas);
    }
    let walkable = frame.shared.walkability.walkable.iter().filter(|w| **w == 1).count();
    println!("  terrain {} walk tiles {} (walkable {}) regions {} neighbors {} buildable {}",
        frame.shared.terrain.buildable.len(), frame.shared.walkability.walkable.len(), walkable,
        frame.shared.regions.id.len(), frame.shared.region_neighbors.region.len(),
        frame.shared.terrain.buildable.iter().filter(|b| **b == 1).count());
    if let Some(game) = frame.game.row(0) {
        println!("  game type {} multiplayer {} paused {} latency {} frames {} frame_ms {} elapsed {} seed {} map {:?} title {:?}",
            game.game_type, game.multiplayer, game.paused, game.latency, game.latency_frames, game.frame_ms,
            game.elapsed_seconds, game.random_seed, game.map_file.as_str(), game.map_title.as_str());
    }
    println!("  start locations {:?}", (0..frame.start_locations.x.len()).map(|i| (frame.start_locations.x[i], frame.start_locations.y[i])).collect::<Vec<_>>());
    println!("  forces {:?}", frame.forces.name.iter().map(|n| n.as_str().to_owned()).collect::<Vec<_>>());
    for index in 0..frame.units.id.len().min(5) {
        println!("  unit {:?}", frame.units.row(index));
    }
    times.sort();
    let at = |q: f64| times[((times.len() - 1) as f64 * q) as usize].as_secs_f64() * 1000.0;
    println!("acquire ms p50 {:.3} p99 {:.3} max {:.3}", at(0.5), at(0.99), at(1.0));
}

#[cfg(not(windows))]
fn main() {}
