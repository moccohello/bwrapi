//! Digests of the tables the in-game and out-of-game reads compare: the
//! compared columns of a table hashed row by row with FNV-1a 64. Both sides
//! of the comparison call the same function.

use bwrapi_engine::tables::{Players, Units};
use bwrapi_engine::Table;

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn mix(mut digest: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        digest ^= u64::from(byte);
        digest = digest.wrapping_mul(FNV_PRIME);
    }
    digest
}

/// Digest of the compared unit columns (id, unit_type, owner, x, y,
/// hit_points, order) in row order.
pub fn units_digest(units: &Units) -> u64 {
    let mut digest = FNV_OFFSET_BASIS;
    for row in 0..units.rows() {
        digest = mix(digest, &units.id[row].to_le_bytes());
        digest = mix(digest, &units.unit_type[row].to_le_bytes());
        digest = mix(digest, &units.owner[row].to_le_bytes());
        digest = mix(digest, &units.x[row].to_le_bytes());
        digest = mix(digest, &units.y[row].to_le_bytes());
        digest = mix(digest, &units.hit_points[row].to_le_bytes());
        digest = mix(digest, &units.order[row].to_le_bytes());
    }
    digest
}

/// Digest of the compared player columns (minerals, gas, the six supply
/// columns) in row order.
pub fn players_digest(players: &Players) -> u64 {
    let mut digest = FNV_OFFSET_BASIS;
    for row in 0..players.rows() {
        digest = mix(digest, &players.minerals[row].to_le_bytes());
        digest = mix(digest, &players.gas[row].to_le_bytes());
        digest = mix(digest, &players.supply_used_zerg[row].to_le_bytes());
        digest = mix(digest, &players.supply_used_terran[row].to_le_bytes());
        digest = mix(digest, &players.supply_used_protoss[row].to_le_bytes());
        digest = mix(digest, &players.supply_total_zerg[row].to_le_bytes());
        digest = mix(digest, &players.supply_total_terran[row].to_le_bytes());
        digest = mix(digest, &players.supply_total_protoss[row].to_le_bytes());
    }
    digest
}

#[cfg(test)]
mod tests {
    use super::*;
    use bwrapi_engine::tables::{PlayerRow, UnitRow};

    #[test]
    fn units_digest_follows_the_compared_columns() {
        let mut units = Units::default();
        units.push(UnitRow {
            id: 7, unit_type: 3, owner: 1, x: 10, y: 20, hit_points: 256, order: 5,
            ..UnitRow::UNKNOWN
        });
        units.push(UnitRow {
            id: 9, unit_type: 4, owner: 2, x: 30, y: 40, hit_points: 512, order: 6,
            ..UnitRow::UNKNOWN
        });
        assert_eq!(units_digest(&units), units_digest(&units.clone()));

        let mut changed = units.clone();
        changed.hit_points[1] += 1;
        assert_ne!(units_digest(&units), units_digest(&changed));

        // A column outside the compared set does not move the digest.
        let mut ignored = units.clone();
        ignored.shields[0] = 100;
        assert_eq!(units_digest(&units), units_digest(&ignored));

        let mut reordered = Units::default();
        reordered.push(units.row(1).unwrap());
        reordered.push(units.row(0).unwrap());
        assert_ne!(units_digest(&units), units_digest(&reordered));
    }

    #[test]
    fn players_digest_follows_the_compared_columns() {
        let mut players = Players::default();
        players.push(PlayerRow {
            minerals: 50, gas: 0, supply_used_terran: 18, supply_total_terran: 20,
            ..PlayerRow::UNKNOWN
        });
        assert_eq!(players_digest(&players), players_digest(&players.clone()));

        let mut changed = players.clone();
        changed.minerals[0] += 1;
        assert_ne!(players_digest(&players), players_digest(&changed));

        let mut ignored = players.clone();
        ignored.name[0] = bwrapi_engine::Text32::from_bytes(b"ignored");
        assert_eq!(players_digest(&players), players_digest(&ignored));
    }
}
