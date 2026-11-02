//! Source readers: each fills one table of the frame from block reads.

mod bullets;
mod game;
mod orders;
mod players;
pub(crate) mod pool;
mod render;
mod terrain;
mod units;
mod upgrades;
mod vision;

use crate::engine::{Frame, State, Target};
use crate::tables::source;

const NEEDS_POOL: u64 =
    source::UNITS | source::BULLETS | source::SELECTION | source::ORDERS | source::UPGRADES | source::TECHS | source::RENDER;

const MATCH_TABLES: u64 = source::TERRAIN | source::WALKABILITY | source::REGIONS | source::REGION_NEIGHBORS;

/// Fills the tables of `sources` and returns the sources that could be read.
pub(crate) fn fill(target: &Target, state: &mut State, sources: u64, frame: &mut Frame) -> u64 {
    let mut read = sources;
    if sources & source::PLAYERS != 0 {
        players::players(target, state, &mut frame.players);
    }
    if sources & source::UNIT_COUNTS != 0 {
        players::unit_counts(target, state, &mut frame.unit_counts);
    }
    if sources & source::GAME != 0 {
        game::game(target, state, &mut frame.game);
    }
    if sources & source::FORCES != 0 {
        game::forces(target, &mut frame.forces);
    }
    if sources & source::START_LOCATIONS != 0 {
        game::start_locations(target, &mut frame.start_locations);
    }
    if sources & source::UNIT_AVAILABILITY != 0 {
        game::unit_availability(target, state, &mut frame.unit_availability);
    }
    if sources & source::VIEWPORT != 0 {
        players::viewport(target, &mut frame.viewport);
    }
    if sources & source::VISION != 0 {
        vision::vision(target, state, frame);
    }
    if sources & MATCH_TABLES != 0 {
        frame.shared = terrain::get(target, state);
    }
    state.pool_base = 0;
    if sources & NEEDS_POOL != 0 {
        pool::read(target, state);
    }
    if sources & (source::UPGRADES | source::TECHS) != 0 {
        upgrades::read_span(target, state);
        upgrades::fill_progress(target, state, false);
        upgrades::fill_progress(target, state, true);
        if let Some(identities) = players::read_identities(target) {
            let ids = players::slot_ids(&identities);
            if sources & source::UPGRADES != 0 {
                upgrades::upgrades(state, &ids, &mut frame.upgrades);
            }
            if sources & source::TECHS != 0 {
                upgrades::techs(state, &ids, &mut frame.techs);
            }
        }
    }
    if sources & source::UNITS != 0 {
        units::units(target, state, &mut frame.units);
    }
    if sources & source::BULLETS != 0 {
        bullets::bullets(target, state, &mut frame.bullets);
    }
    match pool::get(target, state) {
        Some(pool) => {
            if sources & source::SELECTION != 0 && !orders::selection(target, &pool, &mut frame.selection) {
                read &= !source::SELECTION;
            }
            if sources & source::ORDERS != 0 {
                orders::orders(target, &pool, &mut frame.orders);
            }
            if sources & source::RENDER != 0 {
                render::render(target, &pool, &mut frame.render);
            }
        }
        None => read &= !(source::SELECTION | source::ORDERS | source::RENDER),
    }
    read
}

/// The local player id, when the game reports one consistently.
pub(crate) fn local_player(target: &Target) -> Option<u8> {
    let mut raw = [0; crate::decode::scalars::CURRENT_PLAYER_ID_CLUSTER_BYTES];
    target.read_module(target.layout.scalars.current_player_id_cluster_offset, &mut raw).ok()?;
    crate::decode::scalars::read_current_player_id_cluster(&raw)
}

pub(crate) fn u16_at(raw: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([raw[at], raw[at + 1]])
}

pub(crate) fn i16_at(raw: &[u8], at: usize) -> i16 {
    u16_at(raw, at) as i16
}

pub(crate) fn u32_at(raw: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(raw[at..at + 4].try_into().unwrap())
}

pub(crate) fn i32_at(raw: &[u8], at: usize) -> i32 {
    u32_at(raw, at) as i32
}
