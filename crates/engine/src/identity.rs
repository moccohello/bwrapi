//! Unit ids computed from the pool slot and the game's per-slot uniqueness
//! counter, so every session reading the same game names a unit the same
//! way. Within a match an id comes back only after its slot was reused 256
//! times.

/// Unit id: `(slot + 1) | uniqueness << 16`; never 0.
pub fn unit_id(slot: u16, uniqueness: u8) -> u64 {
    (u64::from(slot) + 1) | u64::from(uniqueness) << 16
}

pub fn unit_slot(id: u64) -> u16 {
    (id as u16).wrapping_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_the_slot_and_change_with_uniqueness() {
        assert_ne!(unit_id(0, 0), 0);
        assert_eq!(unit_slot(unit_id(1699, 200)), 1699);
        assert_ne!(unit_id(7, 3), unit_id(7, 4));
    }
}
