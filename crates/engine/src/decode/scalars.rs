/// Session fields decoded from raw bytes; a field with an invalid raw value
/// stays `None`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SessionRuntime {
    pub paused: Option<bool>,
    pub network_in_match: Option<bool>,
    pub latency_raw: Option<u8>,
    pub latency_frames: Option<u32>,
}

pub const SESSION_PAUSED_BYTES: usize = 1;
pub const SESSION_NETWORK_IN_MATCH_BYTES: usize = 1;
pub const GAME_SPEED_PRIMARY_DELAYS_BYTES: usize = 7 * 4;
pub const SUPPLY_TABLE_BYTES: usize = 0x1b0;
pub const PLAYER_IDENTITY_TABLE_BYTES: usize = PLAYER_IDENTITY_ENTRY_STRIDE * 8;
pub const CURRENT_PLAYER_ID_CLUSTER_BYTES: usize = 0x14;

const SUPPLY_ENTRY_STRIDE: usize = 4;
const SUPPLY_ARRAY_ENTRIES: usize = 12;
const SUPPLY_ARRAY_BYTES: usize = SUPPLY_ARRAY_ENTRIES * SUPPLY_ENTRY_STRIDE;
const SUPPLY_ENTRY_COUNT: u8 = 8;
pub const PLAYER_IDENTITY_ENTRY_STRIDE: usize = 0x24;
pub const PLAYER_IDENTITY_ENTRY_COUNT: u8 = 8;
const PLAYER_IDENTITY_ID_OFFSET: usize = 0;
const PLAYER_IDENTITY_TYPE_OFFSET: usize = 8;
const PLAYER_IDENTITY_RACE_OFFSET: usize = 9;
const PLAYER_TYPE_EMPTY: u8 = 0;
const PLAYER_TYPE_COMPUTER: u8 = 1;
const PLAYER_TYPE_HUMAN: u8 = 2;
const PLAYER_TYPE_RESCUE_PASSIVE: u8 = 3;
// Campaign maps keep RescueActive and Neutral (BWAPI PlayerTypes 4 and 7)
// player rows; BWAPI GameEvents lists every type below Closed.
const PLAYER_TYPE_RESCUE_ACTIVE: u8 = 4;
const PLAYER_TYPE_NEUTRAL: u8 = 7;
// BWAPI::PlayerTypes::ComputerLeft.  This terminal in-game row has no
// corresponding active BWGame player slot.
const PLAYER_TYPE_COMPUTER_LEFT: u8 = 11;
const CURRENT_PLAYER_ID_CLUSTER_VALUE_OFFSETS: [usize; 4] = [0x00, 0x04, 0x0c, 0x10];

const RACE_ZERG: u8 = 0;
const RACE_TERRAN: u8 = 1;
const RACE_PROTOSS: u8 = 2;

const ZERG_SUPPLY_AVAILABLE_TABLE: usize = 0x000;
const ZERG_SUPPLY_USED_TABLE: usize = ZERG_SUPPLY_AVAILABLE_TABLE + SUPPLY_ARRAY_BYTES;
const ZERG_SUPPLY_MAX_TABLE: usize = ZERG_SUPPLY_USED_TABLE + SUPPLY_ARRAY_BYTES;
const TERRAN_SUPPLY_AVAILABLE_TABLE: usize = ZERG_SUPPLY_MAX_TABLE + SUPPLY_ARRAY_BYTES;
const TERRAN_SUPPLY_USED_TABLE: usize = TERRAN_SUPPLY_AVAILABLE_TABLE + SUPPLY_ARRAY_BYTES;
const TERRAN_SUPPLY_MAX_TABLE: usize = TERRAN_SUPPLY_USED_TABLE + SUPPLY_ARRAY_BYTES;
const PROTOSS_SUPPLY_AVAILABLE_TABLE: usize = TERRAN_SUPPLY_MAX_TABLE + SUPPLY_ARRAY_BYTES;
const PROTOSS_SUPPLY_USED_TABLE: usize = PROTOSS_SUPPLY_AVAILABLE_TABLE + SUPPLY_ARRAY_BYTES;
const PROTOSS_SUPPLY_MAX_TABLE: usize = PROTOSS_SUPPLY_USED_TABLE + SUPPLY_ARRAY_BYTES;

/// Match-static mapping between an identity-table entry ordinal and the
/// embedded actual player id.  BWGame per-player arrays are addressed by the
/// ordinal; CUnit and local module tables are addressed by `player_id`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlayerSlotBinding {
    pub slot_ordinal: u8,
    pub player_id: u8,
}

impl PlayerSlotBinding {
    pub fn new(slot_ordinal: u8, player_id: u8) -> Option<Self> {
        (slot_ordinal < PLAYER_IDENTITY_ENTRY_COUNT
            && player_id < PLAYER_IDENTITY_ENTRY_COUNT)
            .then_some(Self {
                slot_ordinal,
                player_id,
            })
    }

    pub fn slot_index(self) -> usize {
        usize::from(self.slot_ordinal)
    }

    pub fn actual_player_id(self) -> u8 {
        self.player_id
    }
}

pub fn read_session_runtime_inputs(
    paused_raw: Option<u8>,
    network_in_match_raw: Option<u8>,
    latency_raw: Option<u32>,
    builtin_turn_latency: Option<u32>,
    sync_active: Option<u32>,
) -> Option<SessionRuntime> {
    let paused = paused_raw.and_then(|value| match value {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    });
    let network_in_match = network_in_match_raw.and_then(|value| match value {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    });
    let latency_raw = latency_raw.filter(|value| *value <= 2);
    let latency_frames = builtin_turn_latency
        .zip(sync_active)
        .and_then(|(builtin, sync_active)| {
            if sync_active == 0 {
                Some(builtin)
            } else {
                builtin.checked_add(latency_raw?)
            }
        })
        .filter(|value| i32::try_from(*value).is_ok());
    let latency_raw = latency_raw.map(|value| value as u8);
    if paused.is_none()
        && network_in_match.is_none()
        && latency_raw.is_none()
        && latency_frames.is_none()
    {
        return None;
    }
    Some(SessionRuntime {
        paused,
        network_in_match,
        latency_raw,
        latency_frames,
    })

}

/// Read active identity-table entries and validate their embedded actual ids.
/// Empty and terminal ComputerLeft rows are omitted; duplicate active actual
/// ids fail the whole read so a caller cannot silently choose one row.
pub fn read_player_slot_bindings(raw: &[u8]) -> Option<Vec<PlayerSlotBinding>> {
    if raw.len() < PLAYER_IDENTITY_TABLE_BYTES {
        return None;
    }
    let mut bindings = Vec::new();
    let mut seen_player_ids = [false; PLAYER_IDENTITY_ENTRY_COUNT as usize];
    for slot_ordinal in 0..PLAYER_IDENTITY_ENTRY_COUNT {
        let offset = usize::from(slot_ordinal) * PLAYER_IDENTITY_ENTRY_STRIDE;
        let player_type = *raw.get(offset + PLAYER_IDENTITY_TYPE_OFFSET)?;
        if player_type == PLAYER_TYPE_EMPTY || player_type == PLAYER_TYPE_COMPUTER_LEFT {
            continue;
        }
        if !valid_player_type(player_type) {
            return None;
        }
        let player_id = i32_from(raw, offset + PLAYER_IDENTITY_ID_OFFSET)?;
        if !(0..i32::from(PLAYER_IDENTITY_ENTRY_COUNT)).contains(&player_id) {
            return None;
        }
        let race = *raw.get(offset + PLAYER_IDENTITY_RACE_OFFSET)?;
        if !valid_race(race) {
            return None;
        }
        let player_id = u8::try_from(player_id).ok()?;
        let player_index = usize::from(player_id);
        if seen_player_ids[player_index] {
            return None;
        }
        seen_player_ids[player_index] = true;
        bindings.push(PlayerSlotBinding::new(slot_ordinal, player_id)?);
    }
    Some(bindings)
}

/// Stored actual id only, including inactive rows. This does not establish
/// participation or uniqueness; callers must reject ambiguous slot mappings.
pub fn read_stored_player_slot_id(raw: &[u8], slot: u8) -> Option<u8> {
    if raw.len() < PLAYER_IDENTITY_TABLE_BYTES || slot >= PLAYER_IDENTITY_ENTRY_COUNT { return None; }
    let offset = usize::from(slot) * PLAYER_IDENTITY_ENTRY_STRIDE + PLAYER_IDENTITY_ID_OFFSET;
    let id = u8::try_from(i32_from(raw, offset)?).ok()?;
    (id < PLAYER_IDENTITY_ENTRY_COUNT).then_some(id)
}

pub fn read_current_player_id_cluster(raw: &[u8]) -> Option<u8> {
    if raw.len() < CURRENT_PLAYER_ID_CLUSTER_BYTES {
        return None;
    }

    let mut values = CURRENT_PLAYER_ID_CLUSTER_VALUE_OFFSETS
        .into_iter()
        .map(|offset| i32_from(raw, offset))
        .collect::<Option<Vec<_>>>()?;
    let first = values.pop()?;
    if !(0..i32::from(PLAYER_IDENTITY_ENTRY_COUNT)).contains(&first) {
        return None;
    }
    if values.into_iter().all(|value| value == first) {
        u8::try_from(first).ok()
    } else {
        None
    }
}

/// Exact used-supply rows for all BWAPI playable races.
pub fn read_supply_used_by_race_from_tables(raw: &[u8], player_id: u8) -> Option<[u32; 3]> {
    if raw.len() < SUPPLY_TABLE_BYTES || player_id >= SUPPLY_ENTRY_COUNT {
        return None;
    }
    let mut values = [0; 3];
    for (index, race) in [RACE_ZERG, RACE_TERRAN, RACE_PROTOSS].into_iter().enumerate() {
        let (_, used_base, _) = supply_table_offsets(race)?;
        values[index] = read_supply_entry(raw, used_base, player_id)?;
    }
    Some(values)
}

/// Published supply totals (`min(available, max)`) for all BWAPI playable races.
pub fn read_supply_total_by_race_from_tables(raw: &[u8], player_id: u8) -> Option<[u32; 3]> {
    if raw.len() < SUPPLY_TABLE_BYTES || player_id >= SUPPLY_ENTRY_COUNT {
        return None;
    }
    let mut values = [0; 3];
    for (index, race) in [RACE_ZERG, RACE_TERRAN, RACE_PROTOSS].into_iter().enumerate() {
        let (available_base, _, max_base) = supply_table_offsets(race)?;
        let available = read_supply_entry(raw, available_base, player_id)?;
        values[index] = available.min(read_supply_entry(raw, max_base, player_id)?);
    }
    Some(values)
}

fn i32_from(raw: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_le_bytes(
        raw.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn valid_player_type(value: u8) -> bool {
    matches!(
        value,
        PLAYER_TYPE_EMPTY
            | PLAYER_TYPE_COMPUTER
            | PLAYER_TYPE_HUMAN
            | PLAYER_TYPE_RESCUE_PASSIVE
            | PLAYER_TYPE_RESCUE_ACTIVE
            | PLAYER_TYPE_NEUTRAL
    )
}

fn valid_race(race: u8) -> bool {
    matches!(race, RACE_ZERG | RACE_TERRAN | RACE_PROTOSS)
}

fn supply_table_offsets(race: u8) -> Option<(usize, usize, usize)> {
    match race {
        RACE_ZERG => Some((ZERG_SUPPLY_AVAILABLE_TABLE, ZERG_SUPPLY_USED_TABLE, ZERG_SUPPLY_MAX_TABLE)),
        RACE_TERRAN => Some((TERRAN_SUPPLY_AVAILABLE_TABLE, TERRAN_SUPPLY_USED_TABLE, TERRAN_SUPPLY_MAX_TABLE)),
        RACE_PROTOSS => Some((PROTOSS_SUPPLY_AVAILABLE_TABLE, PROTOSS_SUPPLY_USED_TABLE, PROTOSS_SUPPLY_MAX_TABLE)),
        _ => None,
    }
}

fn read_supply_entry(raw: &[u8], table_base: usize, player_id: u8) -> Option<u32> {
    let offset = table_base + usize::from(player_id) * SUPPLY_ENTRY_STRIDE;
    let bytes = raw.get(offset..offset + SUPPLY_ENTRY_STRIDE)?;
    u32::try_from(i32::from_le_bytes(bytes.try_into().ok()?)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_slot_binding_validates_swapped_ordinals_and_actual_ids() {
        let mut raw = vec![0; PLAYER_IDENTITY_TABLE_BYTES];
        write_player_identity(&mut raw, 0, 1, PLAYER_TYPE_HUMAN, RACE_ZERG);
        write_player_identity(&mut raw, 1, 0, PLAYER_TYPE_COMPUTER, RACE_TERRAN);

        assert_eq!(
            read_player_slot_bindings(&raw),
            Some(vec![
                PlayerSlotBinding::new(0, 1).unwrap(),
                PlayerSlotBinding::new(1, 0).unwrap(),
            ])
        );

        write_player_identity(&mut raw, 2, 0, PLAYER_TYPE_COMPUTER, RACE_PROTOSS);
        assert_eq!(read_player_slot_bindings(&raw), None);
    }

    #[test]
    fn campaign_player_types_are_accepted_and_unknown_ones_rejected() {
        let mut raw = vec![0; PLAYER_IDENTITY_TABLE_BYTES];
        write_player_identity(&mut raw, 1, 1, PLAYER_TYPE_HUMAN, RACE_TERRAN);
        write_player_identity(&mut raw, 2, 2, PLAYER_TYPE_RESCUE_PASSIVE, RACE_TERRAN);
        write_player_identity(&mut raw, 4, 4, PLAYER_TYPE_COMPUTER, RACE_ZERG);
        assert_eq!(read_player_slot_bindings(&raw).unwrap().len(), 3);
        // Brood War Zerg 1 keeps a Neutral (7) row; RescueActive (4) is the
        // other campaign-only type below BWAPI Closed.
        write_player_identity(&mut raw, 0, 0, PLAYER_TYPE_NEUTRAL, RACE_ZERG);
        write_player_identity(&mut raw, 5, 5, PLAYER_TYPE_RESCUE_ACTIVE, RACE_PROTOSS);
        assert_eq!(read_player_slot_bindings(&raw).unwrap().len(), 5);
        write_player_identity(&mut raw, 5, 5, 8, RACE_PROTOSS);
        assert_eq!(read_player_slot_bindings(&raw), None);
        write_player_identity(&mut raw, 5, 5, 0, RACE_PROTOSS);
        write_player_identity(&mut raw, 2, 2, 12, RACE_TERRAN);
        assert_eq!(read_player_slot_bindings(&raw), None);
        write_player_identity(&mut raw, 2, 1, PLAYER_TYPE_RESCUE_PASSIVE, RACE_TERRAN);
        assert_eq!(read_player_slot_bindings(&raw), None);
    }

    #[test]
    fn current_player_id_cluster_requires_matching_valid_values() {
        let mut raw = vec![0; CURRENT_PLAYER_ID_CLUSTER_BYTES];
        for offset in CURRENT_PLAYER_ID_CLUSTER_VALUE_OFFSETS {
            raw[offset..offset + 4].copy_from_slice(&4_i32.to_le_bytes());
        }

        assert_eq!(read_current_player_id_cluster(&raw), Some(4));

        raw[0x0c..0x10].copy_from_slice(&3_i32.to_le_bytes());
        assert_eq!(read_current_player_id_cluster(&raw), None);

        raw[0x0c..0x10].copy_from_slice(&4_i32.to_le_bytes());
        raw[0x10..0x14].copy_from_slice(&8_i32.to_le_bytes());
        assert_eq!(read_current_player_id_cluster(&raw), None);
        assert_eq!(
            read_current_player_id_cluster(&raw[..CURRENT_PLAYER_ID_CLUSTER_BYTES - 1]),
            None
        );
    }

    fn write_player_identity(raw: &mut [u8], slot: u8, player_id: u8, player_type: u8, race: u8) {
        let offset = usize::from(slot) * PLAYER_IDENTITY_ENTRY_STRIDE;
        raw[offset + PLAYER_IDENTITY_ID_OFFSET..offset + PLAYER_IDENTITY_ID_OFFSET + 4]
            .copy_from_slice(&i32::from(player_id).to_le_bytes());
        raw[offset + PLAYER_IDENTITY_TYPE_OFFSET] = player_type;
        raw[offset + PLAYER_IDENTITY_RACE_OFFSET] = race;
    }
}
