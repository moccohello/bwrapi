use crate::decode::scalars::PlayerSlotBinding;

pub const BWGAME_FRAME_COUNT_OFFSET: usize = 0x14c;
pub const BWGAME_MINERALS_OFFSET: usize = 0x00;
pub const BWGAME_GAS_OFFSET: usize = 0x30;
pub const BWGAME_CUMULATIVE_GAS_OFFSET: usize = 0x60;
pub const BWGAME_CUMULATIVE_MINERALS_OFFSET: usize = 0x90;
pub const BWGAME_ELAPSED_TIME_OFFSET: usize = 0xe608;
pub const BWGAME_ROOT_READ_BYTES: usize = 0x10351;
pub const BWGAME_METADATA_READ_BYTES: usize = 0xe650;
/// Canonical [race][available/used/max][12 player slots] supply arrays.
pub const BWGAME_SUPPLY_TABLE_OFFSET: usize = 0x3054;
pub const BWGAME_MAP_FILE_NAME_OFFSET: usize = 0x0c4c;
pub const BWGAME_MAP_FILE_NAME_BYTES: usize = 260;
pub const BWGAME_MAP_TITLE_OFFSET: usize = 0x0d50;
pub const BWGAME_MAP_TITLE_BYTES: usize = 32;
pub const BWGAME_PLAYER_COLOR_INDEX_OFFSET: usize = 0x2ce6;
pub const BWGAME_PLAYER_SCORE_UNIT_OFFSET: usize = 0x2de4;
pub const BWGAME_PLAYER_SCORE_KILL_OFFSET: usize = 0x2e14;
pub const BWGAME_PLAYER_SCORE_BUILDING_OFFSET: usize = 0x2f34;
pub const BWGAME_PLAYER_SCORE_RAZING_OFFSET: usize = 0x2f64;
pub const BWGAME_PLAYER_SCORE_CUSTOM_OFFSET: usize = 0x3204;
pub const BWGAME_PLAYER_UNIT_AVAILABILITY_OFFSET: usize = 0x018c;
pub const BWGAME_UNIT_AVAILABILITY_TYPE_COUNT: usize = 228;
pub const BWGAME_FORCE_NAMES_OFFSET: usize = 0xe4cc;
pub const BWGAME_FORCE_NAME_BYTES: usize = 30;
pub const BWGAME_PLAYER_ALLIANCES_OFFSET: usize = 0xe544;
const BWGAME_PLAYER_VISION_OFFSET: usize = 0x0fc;
pub const BWGAME_ALLIANCE_PLAYER_SLOTS: usize = 12;
pub const BWGAME_COUNTDOWN_SECONDS_OFFSET: usize = 0xe604;
pub const BWGAME_PLAYER_VICTORY_STATUS_OFFSET: usize = 0xe610;
/// `BWGame::playerLeft[8]`. This is a canonical C++ bool table addressed by
/// validated BWGame slot ordinal, never by the published actual player id.
pub const BWGAME_PLAYER_LEFT_OFFSET: usize = 0x2c72;
pub const BWGAME_START_POSITIONS_OFFSET: usize = 0xe630;
pub const BWGAME_PLAYABLE_PLAYER_COUNT: usize = 8;

pub const BWGAME_TECH_AVAILABLE_SC_OFFSET: usize = 0xdd34;
pub const BWGAME_TECH_RESEARCHED_SC_OFFSET: usize = 0xde54;
/// SC-era research-in-progress table: three packed bytes per player for 24
/// technology slots. This is the direct companion of the SC available and
/// researched tables; it is not the BW six-byte bitset below.
pub const BWGAME_MAX_UPGRADE_SC_OFFSET: usize = 0xdf98;
pub const BWGAME_CURRENT_UPGRADE_SC_OFFSET: usize = 0xe1c0;
pub const BWGAME_TECH_AVAILABLE_BW_OFFSET: usize = 0xff60;
pub const BWGAME_TECH_RESEARCHED_BW_OFFSET: usize = 0x10050;
pub const BWGAME_TECH_RESEARCHING_BITSET_OFFSET: usize = 0x10140;
pub const BWGAME_MAX_UPGRADE_BW_OFFSET: usize = 0x10188;
pub const BWGAME_CURRENT_UPGRADE_BW_OFFSET: usize = 0x1023c;
/// Packed per-slot upgrade-in-progress bits. Each player owns eight
/// bytes, enough for the complete SC/BW upgrade id range `0..=60`.
pub const BWGAME_UPGRADE_IN_PROGRESS_OFFSET: usize = 0x102f0;
pub const BWGAME_UPGRADE_IN_PROGRESS_STRIDE: usize = 8;

// `BWGame::unitCounts` is four adjacent [unit type][player] i32 tables.
pub const BWGAME_UNIT_COUNT_ALL_OFFSET: usize = 0x3234;
pub const BWGAME_UNIT_COUNT_COMPLETED_OFFSET: usize = 0x5cf4;
pub const BWGAME_UNIT_COUNT_KILLED_OFFSET: usize = 0x87b4;
pub const BWGAME_UNIT_COUNT_DEAD_OFFSET: usize = 0xb274;
pub const BWGAME_UNIT_COUNT_TYPE_COUNT: usize = 228;
pub const BWGAME_UNIT_COUNT_PLAYER_SLOTS: usize = 12;
pub const BWGAME_UNIT_COUNT_ENTRY_BYTES: usize = std::mem::size_of::<i32>();
pub const BWGAME_UNIT_COUNT_ROW_BYTES: usize =
    BWGAME_UNIT_COUNT_PLAYER_SLOTS * BWGAME_UNIT_COUNT_ENTRY_BYTES;
pub const BWGAME_UNIT_COUNT_TABLE_BYTES: usize =
    BWGAME_UNIT_COUNT_TYPE_COUNT * BWGAME_UNIT_COUNT_ROW_BYTES;

pub const BWGAME_PLAYER_COUNT: usize = 8;
const BWGAME_RESOURCE_PLAYER_COUNT: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BwgamePlayerResources {
    pub player_id: u8,
    pub minerals: i32,
    pub gas: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BwgamePlayerGatheredResources {
    pub player_id: u8,
    pub gathered_minerals: i32,
    pub gathered_gas: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BwgameCommandControlState {
    pub local_player_id: u8,
    pub released_player_mask: u16,
    pub alliance_mask: u32,
    pub vision_mask: u16,
    /// Public actual id -> native BWGame/packet slot. Inactive ambiguous ids
    /// remain absent without preventing preservation of the raw packet bits.
    pub target_slots: [Option<u8>; 8],
    pub alliance_known_mask: u8,
}

/// Observe the complete native packet row and separately map public actual ids.
/// Released bindings establish active mappings. Other stored identities are
/// usable only when unique; their absence never replaces native packet bits.
pub fn read_bwgame_command_control_state(
    raw: &[u8],
    bindings: &[PlayerSlotBinding],
    identities: &[u8],
    local_player_id: u8,
) -> Option<BwgameCommandControlState> {
    if raw.len() < BWGAME_METADATA_READ_BYTES || !valid_bindings(bindings) {
        return None;
    }
    let local = bindings
        .iter()
        .find(|binding| binding.actual_player_id() == local_player_id)?;
    let local_slot = local.slot_index();
    let vision_offset = BWGAME_PLAYER_VISION_OFFSET.checked_add(local_slot.checked_mul(4)?)?;
    // The native SetVision packet carries the low 16 bits, independently of
    // whether inactive slots have a publishable actual-id mapping.
    let vision_mask = u32_at(raw, vision_offset)? as u16;
    let mut released_player_mask = 0u16;
    let mut alliance_mask = 0u32;
    let row_offset = BWGAME_PLAYER_ALLIANCES_OFFSET + local_slot * BWGAME_ALLIANCE_PLAYER_SLOTS;
    for slot in 0..BWGAME_ALLIANCE_PLAYER_SLOTS {
        // SetAllies is a native-slot mask of all 12 entries. Its encoder uses
        // each stored value's low two bits, including reserved mode 3.
        alliance_mask |= u32::from(*raw.get(row_offset + slot)? & 3) << (slot * 2);
    }
    let mut target_slots = [None; 8];
    for binding in bindings {
        if crate::decode::scalars::read_stored_player_slot_id(identities, binding.slot_ordinal)
            != Some(binding.actual_player_id()) { return None; }
        released_player_mask |= 1u16 << binding.actual_player_id();
        target_slots[usize::from(binding.actual_player_id())] = Some(binding.slot_ordinal);
    }
    for actual_id in 0u8..8 {
        if target_slots[usize::from(actual_id)].is_some() { continue; }
        let mut candidates = (0u8..8).filter(|slot|
            !bindings.iter().any(|binding| binding.slot_ordinal == *slot)
                && crate::decode::scalars::read_stored_player_slot_id(identities, *slot) == Some(actual_id));
        let first = candidates.next();
        if candidates.next().is_none() { target_slots[usize::from(actual_id)] = first; }
    }
    let alliance_known_mask = target_slots.iter().enumerate().fold(0u8, |known, (id, slot)| {
        known | if slot.is_some_and(|slot| raw[row_offset + usize::from(slot)] <= 2) { 1 << id } else { 0 }
    });
    Some(BwgameCommandControlState {
        local_player_id,
        released_player_mask,
        alliance_mask,
        vision_mask,
        target_slots,
        alliance_known_mask,
    })
}

/// BWGame start positions are tile centres ((tile*32)+64, (tile*32)+48);
/// (0, 0) marks an unused slot.
pub fn canonical_start_tile(x: i16, y: i16, map_dimensions: Option<(u32, u32)>) -> Option<(i32, i32)> {
    if x == 0 && y == 0 {
        return None;
    }
    let pixel_x = i32::from(x) - 64;
    let pixel_y = i32::from(y) - 48;
    if pixel_x.rem_euclid(32) != 0 || pixel_y.rem_euclid(32) != 0 {
        return None;
    }
    let (tile_x, tile_y) = (pixel_x / 32, pixel_y / 32);
    let (map_width, map_height) = map_dimensions?;
    (tile_x >= 0 && tile_y >= 0 && (tile_x as u32) < map_width && (tile_y as u32) < map_height)
        .then_some((tile_x, tile_y))
}

fn valid_bindings(bindings: &[PlayerSlotBinding]) -> bool {
    let mut slots = [false; BWGAME_PLAYER_COUNT];
    let mut player_ids = [false; BWGAME_PLAYER_COUNT];
    bindings.iter().all(|binding| {
        let slot = binding.slot_index();
        let player_id = usize::from(binding.actual_player_id());
        slot < BWGAME_PLAYER_COUNT
            && player_id < BWGAME_PLAYER_COUNT
            && !std::mem::replace(&mut slots[slot], true)
            && !std::mem::replace(&mut player_ids[player_id], true)
    })
}

pub fn read_current_resources(
    raw: &[u8],
    binding: PlayerSlotBinding,
) -> Option<BwgamePlayerResources> {
    let player_index = binding.slot_index();
    if player_index >= BWGAME_RESOURCE_PLAYER_COUNT {
        return None;
    }
    let minerals = i32_at(raw, BWGAME_MINERALS_OFFSET + player_index * 4)?;
    let gas = i32_at(raw, BWGAME_GAS_OFFSET + player_index * 4)?;
    (minerals >= 0 && gas >= 0).then_some(BwgamePlayerResources {
        player_id: binding.actual_player_id(),
        minerals,
        gas,
    })
}

pub fn read_gathered_resources(
    raw: &[u8],
    binding: PlayerSlotBinding,
) -> Option<BwgamePlayerGatheredResources> {
    let player_index = binding.slot_index();
    if player_index >= BWGAME_RESOURCE_PLAYER_COUNT {
        return None;
    }
    let gathered_gas = i32_at(raw, BWGAME_CUMULATIVE_GAS_OFFSET + player_index * 4)?;
    let gathered_minerals = i32_at(raw, BWGAME_CUMULATIVE_MINERALS_OFFSET + player_index * 4)?;
    (gathered_minerals >= 0 && gathered_gas >= 0).then_some(BwgamePlayerGatheredResources {
        player_id: binding.actual_player_id(),
        gathered_minerals,
        gathered_gas,
    })
}

fn i32_at(raw: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_le_bytes(
        raw.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn u32_at(raw: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(raw.get(offset..offset + 4)?.try_into().ok()?))
}

pub const BWGAME_SC_UPGRADE_COUNT: usize = 46;
pub const BWGAME_BW_UPGRADE_COUNT: usize = 15;
pub const BWGAME_TOTAL_UPGRADE_COUNT: usize = BWGAME_SC_UPGRADE_COUNT + BWGAME_BW_UPGRADE_COUNT;
pub const BWGAME_SC_TECH_COUNT: usize = 24;
pub const BWGAME_BW_TECH_COUNT: usize = 20;
pub const BWGAME_TOTAL_TECH_COUNT: usize = BWGAME_SC_TECH_COUNT + BWGAME_BW_TECH_COUNT;
pub const BWGAME_TECH_BITSET_STRIDE: usize = 6;
pub const BWGAME_MAX_UPGRADE_LEVEL: u8 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_state_maps_all_actual_targets_and_preserves_all_native_packet_bits() {
        let mut raw = vec![0; BWGAME_METADATA_READ_BYTES];
        let mut identities = vec![0; crate::decode::scalars::PLAYER_IDENTITY_TABLE_BYTES];
        for (slot, id) in [1i32, 0, 2, 3, 4, 5, 6, 7].iter().enumerate() {
            let start = slot * crate::decode::scalars::PLAYER_IDENTITY_ENTRY_STRIDE;
            identities[start..start + 4].copy_from_slice(&id.to_le_bytes());
        }
        let bindings = [PlayerSlotBinding::new(0, 1).unwrap(), PlayerSlotBinding::new(1, 0).unwrap()];
        let modes = [2u8, 1, 3, 0, 2, 1, 0, 2, 1, 2, 3, 0];
        let offset = BWGAME_PLAYER_ALLIANCES_OFFSET + BWGAME_ALLIANCE_PLAYER_SLOTS;
        raw[offset..offset + 12].copy_from_slice(&modes);
        raw[BWGAME_PLAYER_VISION_OFFSET + 4..BWGAME_PLAYER_VISION_OFFSET + 8]
            .copy_from_slice(&0x8101u32.to_le_bytes());
        let state = read_bwgame_command_control_state(&raw, &bindings, &identities, 0).unwrap();
        assert_eq!(state.target_slots, [Some(1), Some(0), Some(2), Some(3), Some(4), Some(5), Some(6), Some(7)]);
        assert_eq!(state.released_player_mask, 3);
        assert_eq!(state.alliance_known_mask, 0xff & !(1 << 2));
        assert_eq!(state.alliance_mask, modes.iter().enumerate().fold(0, |mask, (slot, value)| mask | u32::from(*value) << (slot * 2)));
        assert_eq!(state.vision_mask, 0x8101);
    }

    #[test]
    fn inactive_identity_unknown_does_not_invent_settings_or_destroy_packet_source() {
        let mut raw = vec![0; BWGAME_METADATA_READ_BYTES];
        let mut identities = vec![0xff; crate::decode::scalars::PLAYER_IDENTITY_TABLE_BYTES];
        identities[..4].copy_from_slice(&0i32.to_le_bytes());
        let stride = crate::decode::scalars::PLAYER_IDENTITY_ENTRY_STRIDE;
        identities[stride..stride + 4].copy_from_slice(&1i32.to_le_bytes());
        // An inactive row's stale id duplicates the released local player's id.
        identities[2 * stride..2 * stride + 4].copy_from_slice(&0i32.to_le_bytes());
        let bindings = [PlayerSlotBinding::new(0, 0).unwrap(), PlayerSlotBinding::new(1, 1).unwrap()];
        raw[BWGAME_PLAYER_ALLIANCES_OFFSET + 1] = 1;
        raw[BWGAME_PLAYER_ALLIANCES_OFFSET + 7] = 2;
        raw[BWGAME_PLAYER_ALLIANCES_OFFSET + 11] = 3;
        let state = read_bwgame_command_control_state(&raw, &bindings, &identities, 0).unwrap();
        assert_eq!(state.target_slots, [Some(0), Some(1), None, None, None, None, None, None]);
        assert_eq!(state.alliance_known_mask, 3);
        assert_eq!((state.alliance_mask >> 14) & 3, 2);
        assert_eq!((state.alliance_mask >> 22) & 3, 3);
        assert!(read_bwgame_command_control_state(&raw, &bindings, &[], 0).is_none());
    }
}
