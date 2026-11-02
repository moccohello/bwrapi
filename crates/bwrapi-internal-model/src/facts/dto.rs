use crate::adapted::{Position, ValidationStatus, Visibility};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceAddress {
    Int(u64),
    Text(String),
}

impl From<u64> for SourceAddress {
    fn from(value: u64) -> Self {
        Self::Int(value)
    }
}

impl From<usize> for SourceAddress {
    fn from(value: usize) -> Self {
        Self::Int(value as u64)
    }
}

impl From<&str> for SourceAddress {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for SourceAddress {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedOrderFact {
    pub order_id: Option<u16>,
    pub unit_type_id: Option<u16>,
    pub target_position: Option<Position>,
    pub target_unit_tag: Option<String>,
    pub secondary_order_target_position: Option<Position>,
    pub move_target_position: Option<Position>,
    pub move_target_unit_tag: Option<String>,
    pub status: ValidationStatus,
}

impl Default for QueuedOrderFact {
    fn default() -> Self {
        Self {
            order_id: None,
            unit_type_id: None,
            target_position: None,
            target_unit_tag: None,
            secondary_order_target_position: None,
            move_target_position: None,
            move_target_unit_tag: None,
            status: ValidationStatus::Pending,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitOrderFact {
    pub order_id: Option<u16>,
    pub secondary_order_id: Option<u16>,
    pub order_timer: Option<u8>,
    pub target_position: Option<Position>,
    pub target_position_available: Option<bool>,
    pub target_unit_tag: Option<String>,
    pub target_unit_available: Option<bool>,
    pub secondary_order_target_position: Option<Position>,
    pub move_target_position: Option<Position>,
    pub move_target_unit_tag: Option<String>,
    pub next_movement_waypoint: Option<Position>,
    pub next_target_waypoint: Option<Position>,
    pub rally_position: Option<Position>,
    pub rally_position_available: Option<bool>,
    pub rally_unit_tag: Option<String>,
    pub rally_unit_available: Option<bool>,
    pub queue_count: Option<u8>,
    pub queued_orders: Vec<QueuedOrderFact>,
    pub status: ValidationStatus,
}

impl Default for UnitOrderFact {
    fn default() -> Self {
        Self {
            order_id: None,
            secondary_order_id: None,
            order_timer: None,
            target_position: None,
            target_position_available: None,
            target_unit_tag: None,
            target_unit_available: None,
            secondary_order_target_position: None,
            move_target_position: None,
            move_target_unit_tag: None,
            next_movement_waypoint: None,
            next_target_waypoint: None,
            rally_position: None,
            rally_position_available: None,
            rally_unit_tag: None,
            rally_unit_available: None,
            queue_count: None,
            queued_orders: Vec::new(),
            status: ValidationStatus::Pending,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitVitalFact {
    pub shields: Option<i32>,
    pub hit_points_raw: Option<i32>,
    pub shields_raw: Option<i32>,
    pub energy_raw: Option<i32>,
    pub status: ValidationStatus,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnitMotionFact {
    pub is_moving: Option<bool>,
    pub is_accelerating: Option<bool>,
    pub is_braking: Option<bool>,
    pub facing_direction_raw: Option<u8>,
    pub turn_radius_raw: Option<u8>,
    pub velocity_direction_raw: Option<u8>,
    pub velocity_x_raw: Option<i32>,
    pub velocity_y_raw: Option<i32>,
    pub flingy_id: Option<u16>, pub flingy_movement_type: Option<u8>, pub halt_position_raw: Option<Position>,
    pub flingy_top_speed_raw: Option<u32>, pub flingy_current_speed1_raw: Option<i32>, pub flingy_current_speed2_raw: Option<i32>,
    pub flingy_acceleration: Option<u16>, pub current_direction2_raw: Option<u8>, pub velocity_direction2_raw: Option<u8>,
}

impl Default for UnitVitalFact {
    fn default() -> Self {
        Self {
            shields: None,
            hit_points_raw: None,
            shields_raw: None,
            energy_raw: None,
            status: ValidationStatus::Pending,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitProductionFact {
    pub is_completed: Option<bool>,
    pub build_type_id: Option<u16>,
    pub build_unit_tag: Option<String>,
    pub addon_unit_tag: Option<String>,
    pub is_powered: Option<bool>,
    pub training_queue: Option<Vec<u16>>,
    pub tech_type_id: Option<u16>,
    pub upgrade_type_id: Option<u16>,
    pub remaining_build_frames: Option<u32>,
    pub remaining_train_frames: Option<u32>,
    pub remaining_research_frames: Option<u32>,
    pub remaining_upgrade_frames: Option<u32>,
    pub landing_timer: Option<u8>,
    pub upgrade_level: Option<u8>,
    pub status: ValidationStatus,
}

impl Default for UnitProductionFact {
    fn default() -> Self {
        Self {
            is_completed: None,
            build_type_id: None,
            build_unit_tag: None,
            addon_unit_tag: None,
            is_powered: None,
            training_queue: None,
            tech_type_id: None,
            upgrade_type_id: None,
            remaining_build_frames: None,
            remaining_train_frames: None,
            remaining_research_frames: None,
            remaining_upgrade_frames: None,
            landing_timer: None,
            upgrade_level: None,
            status: ValidationStatus::Pending,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitCunitFact {
    pub order_queue_timer: Option<u8>, pub attack_notify_timer: Option<u8>, pub previous_unit_type_id: Option<u16>, pub last_event_timer: Option<u8>, pub last_event_color: Option<u8>, pub rank_increase: Option<u8>, pub secondary_order_timer: Option<u8>, pub ai_action_flags: Option<u8>, pub user_action_flags: Option<u8>, pub button_set_id: Option<u16>, pub raw_is_cloaked: Option<bool>, pub movement_state: Option<u8>, pub recently_attacked: Option<bool>, pub building_overlay_state: Option<u8>, pub hit_points_gain_raw: Option<u16>, pub shield_gain_raw: Option<u16>, pub previous_hit_points_raw: Option<u16>,
    pub ground_weapon_cooldown: Option<u8>,
    pub air_weapon_cooldown: Option<u8>,
    pub spell_cooldown: Option<u8>,
    pub kill_count: Option<u8>,
    pub acid_spore_count: Option<u8>,
    pub remove_timer: Option<u16>,
    pub defense_matrix_damage_raw: Option<u16>,
    pub defense_matrix_timer: Option<u8>,
    pub stim_timer: Option<u8>,
    pub cargo_unit_tags: Vec<String>,
    pub cargo_space_total: Option<u8>, pub cargo_space_used: Option<u8>, pub cargo_space_remaining: Option<u8>,
    pub ensnare_timer: Option<u8>,
    pub lockdown_timer: Option<u8>,
    pub irradiate_timer: Option<u8>,
    pub stasis_timer: Option<u8>,
    pub plague_timer: Option<u8>,
    pub storm_timer: Option<u8>,
    pub maelstrom_timer: Option<u8>,
    pub has_sub_unit: Option<bool>,
    pub sub_unit_tag: Option<String>,
    pub has_resource_target: Option<bool>,
    pub resource_target_unit_tag: Option<String>,
    pub has_power_up: Option<bool>,
    pub power_up_unit_tag: Option<String>,
    pub is_carrying_resource: Option<bool>,
    pub resource_carry_count: Option<u8>,
    pub is_gathering: Option<bool>,
    pub is_being_gathered: Option<bool>,
    pub resource_group: Option<u8>,
    pub is_in_bunker: Option<bool>,
    pub bunker_unit_tag: Option<String>,
    pub has_hatchery_parent: Option<bool>,
    pub hatchery_unit_tag: Option<String>,
    pub nydus_exit_unit_tag: Option<String>,
    pub is_interruptible: Option<bool>,
    pub is_invincible: Option<bool>,
    pub is_stuck: Option<bool>,
    pub has_nuke: Option<bool>,
    pub is_blind: Option<bool>,
    pub is_parasited: Option<bool>,
    pub has_permanent_addon: Option<bool>,
    pub permanent_addon_unit_tag: Option<String>,
    pub addon_build_type_id: Option<u16>,
    pub interceptor_count: Option<u8>,
    pub scarab_count: Option<u8>,
    pub has_hangar_parent: Option<bool>,
    pub hangar_parent_unit_tag: Option<String>,
    pub is_in_hangar: Option<bool>,
    pub hangar_children_complete: Option<bool>,
    pub resource_cargo_kind: Option<u8>,
    pub is_being_healed: Option<bool>,
    pub last_attacking_player_id: Option<u8>,
    pub spider_mine_count: Option<u8>,
    pub gather_queue_count: Option<u8>,
    pub status: ValidationStatus,
}

impl Default for UnitCunitFact {
    fn default() -> Self {
        Self {
            order_queue_timer: None, attack_notify_timer: None, previous_unit_type_id: None, last_event_timer: None, last_event_color: None, rank_increase: None, secondary_order_timer: None, ai_action_flags: None, user_action_flags: None, button_set_id: None, raw_is_cloaked: None, movement_state: None, recently_attacked: None, building_overlay_state: None, hit_points_gain_raw: None, shield_gain_raw: None, previous_hit_points_raw: None,
            ground_weapon_cooldown: None,
            air_weapon_cooldown: None,
            spell_cooldown: None,
            kill_count: None,
            acid_spore_count: None,
            remove_timer: None,
            defense_matrix_damage_raw: None,
            defense_matrix_timer: None,
            stim_timer: None,
            cargo_unit_tags: Vec::new(),
            cargo_space_total: None, cargo_space_used: None, cargo_space_remaining: None,
            ensnare_timer: None,
            lockdown_timer: None,
            irradiate_timer: None,
            stasis_timer: None,
            plague_timer: None,
            storm_timer: None,
            maelstrom_timer: None,
            has_sub_unit: None,
            sub_unit_tag: None,
            has_resource_target: None,
            resource_target_unit_tag: None,
            has_power_up: None,
            power_up_unit_tag: None,
            is_carrying_resource: None,
            resource_carry_count: None,
            is_gathering: None,
            is_being_gathered: None,
            resource_group: None,
            is_in_bunker: None,
            bunker_unit_tag: None,
            has_hatchery_parent: None,
            hatchery_unit_tag: None,
            nydus_exit_unit_tag: None,
            is_interruptible: None,
            is_invincible: None,
            is_stuck: None,
            has_nuke: None,
            is_blind: None,
            is_parasited: None,
            has_permanent_addon: None,
            permanent_addon_unit_tag: None,
            addon_build_type_id: None,
            interceptor_count: None,
            scarab_count: None,
            has_hangar_parent: None,
            hangar_parent_unit_tag: None,
            is_in_hangar: None,
            hangar_children_complete: None,
            resource_cargo_kind: None, is_being_healed: None, last_attacking_player_id: None, spider_mine_count: None,
            gather_queue_count: None,
            status: ValidationStatus::Pending,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitFact {
    pub tag: Option<String>,
    pub source_namespace: String,
    pub source_address: Option<SourceAddress>,
    pub record_index: Option<usize>,
    pub owner_id: Option<u8>,
    pub unit_type_id: Option<u16>,
    pub position: Option<Position>,
    pub order: Option<UnitOrderFact>,
    pub vitals: Option<UnitVitalFact>,
    pub production: Option<UnitProductionFact>,
    pub cunit: Option<UnitCunitFact>,
    pub transport_unit_tag: Option<String>,
    pub is_loaded: Option<bool>,
    pub in_air: Option<bool>,
    pub is_burrowed: Option<bool>,
    pub is_cloaked: Option<bool>,
    pub requires_detection: Option<bool>,
    pub detection_player_flags: Option<u32>,
    pub motion: Option<UnitMotionFact>,
    pub is_hallucination: Option<bool>,
    pub visibility: Visibility,
    pub status: ValidationStatus,
}

impl Default for UnitFact {
    fn default() -> Self {
        Self {
            tag: None,
            source_namespace: "unit".to_owned(),
            source_address: None,
            record_index: None,
            owner_id: None,
            unit_type_id: None,
            position: None,
            order: None,
            vitals: None,
            production: None,
            cunit: None,
            transport_unit_tag: None,
            is_loaded: None,
            in_air: None,
            is_burrowed: None,
            is_cloaked: None,
            requires_detection: None,
            detection_player_flags: None,
            motion: None,
            is_hallucination: None,
            visibility: Visibility::Unknown,
            status: ValidationStatus::Pending,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpgradeFact {
    pub player_id: u8,
    pub upgrade_type_id: u16,
    pub level: Option<u8>,
    pub max_level: Option<u8>,
    pub in_progress: Option<bool>,
    pub remaining_frames: Option<u32>,
    pub status: ValidationStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TechFact {
    pub player_id: u8,
    pub tech_type_id: u16,
    pub researched: Option<bool>,
    pub available: Option<bool>,
    pub in_progress: Option<bool>,
    pub remaining_frames: Option<u32>,
    pub status: ValidationStatus,
}
