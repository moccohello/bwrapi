//! BWAPI GameEvents match-start participation, from complete native inputs.
//! Trigger lists must already be assigned to their native execution players.
//! CHK owner flags alone are not an executor-list binding.

pub const PLAYERS: usize = 8;
pub const UNIT_TYPES: usize = 228;
pub const TRIGGER_BYTES: usize = 2400;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParticipationMask {
    pub known: u8,
    pub participating: u8,
}

/// Inputs must belong to the same match-start identity/generation. Counts are
/// full engine tables in actual-player order, not visible units or own counts.
/// Missing inputs leave the entire mask unknown; known-empty lists are valid.
pub fn read_participation(
    initial_counts: Option<&[[u32; UNIT_TYPES]; PLAYERS]>,
    triggers_by_executor: Option<&[Vec<&[u8]>; PLAYERS]>,
    teams: Option<&[u8; PLAYERS]>,
) -> ParticipationMask {
    let (Some(counts), Some(triggers), Some(teams)) =
        (initial_counts, triggers_by_executor, teams)
    else {
        return ParticipationMask::default();
    };
    if triggers.iter().flatten().any(|record| record.len() != TRIGGER_BYTES) {
        return ParticipationMask::default();
    }
    let mut result = ParticipationMask { known: u8::MAX, participating: 0 };
    for (player, row) in counts.iter().enumerate() {
        if row.iter().any(|count| *count != 0) {
            result.participating |= 1 << player;
        }
    }
    for (executor, records) in triggers.iter().enumerate() {
        for record in records {
            if !conditions_can_be_met(record) { continue; }
            for player in 0..PLAYERS {
                if actions_allow_gameplay(record, executor, player, teams) {
                    result.participating |= 1 << player;
                }
            }
        }
    }
    result
}

fn conditions_can_be_met(record: &[u8]) -> bool {
    for condition in record[..320].chunks_exact(20) {
        if condition[15] == 23 || condition[17] & 2 != 0 { return false; }
        if condition[15] == 0 { break; }
    }
    true
}

fn actions_allow_gameplay(record: &[u8], executor: usize, player: usize, teams: &[u8; PLAYERS]) -> bool {
    for action in record[320..2368].chunks_exact(32) {
        let kind = action[26];
        if action[28] & 2 == 0 {
            let offset = match kind { 11 | 44 => Some(16), 48 => Some(20), _ => None };
            if let Some(offset) = offset {
                let group = u32::from_le_bytes(action[offset..offset + 4].try_into().unwrap());
                if player_can_match(group, executor, player, teams) { return true; }
            }
        }
        if kind == 0 { break; }
    }
    false
}

fn player_can_match(group: u32, executor: usize, player: usize, teams: &[u8; PLAYERS]) -> bool {
    match group {
        0..=7 => group as usize == player,
        17 | 26 => true,
        14 | 15 => executor != player,
        13 => executor == player,
        18..=21 => u32::from(teams[player]) == group - 18 + 1,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_input_is_unknown_and_complete_empty_is_known_false() {
        let counts = [[0; UNIT_TYPES]; PLAYERS];
        let lists: [Vec<&[u8]>; PLAYERS] = std::array::from_fn(|_| Vec::new());
        let teams = [0; PLAYERS];
        assert_eq!(read_participation(Some(&counts), None, Some(&teams)).known, 0);
        assert_eq!(read_participation(Some(&counts), Some(&lists), Some(&teams)),
            ParticipationMask { known: 255, participating: 0 });
    }
    #[test]
    fn zero_units_can_participate_via_give_units_force_and_disabled_never_blocks() {
        let counts = [[0; UNIT_TYPES]; PLAYERS];
        let teams = [0, 0, 2, 0, 0, 0, 0, 0];
        let mut record = [0; TRIGGER_BYTES];
        record[320 + 26] = 48;
        record[320 + 20..320 + 24].copy_from_slice(&19u32.to_le_bytes());
        let mut lists: [Vec<&[u8]>; PLAYERS] = std::array::from_fn(|_| Vec::new());
        lists[0].push(&record);
        assert_eq!(read_participation(Some(&counts), Some(&lists), Some(&teams)).participating, 4);
        drop(lists);
        record[15] = 23;
        let mut lists: [Vec<&[u8]>; PLAYERS] = std::array::from_fn(|_| Vec::new());
        lists[0].push(&record);
        assert_eq!(read_participation(Some(&counts), Some(&lists), Some(&teams)).participating, 0);
    }
}
