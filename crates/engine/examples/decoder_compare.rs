//! One-off check of the engine's unit columns and order queues against the
//! reference CUnit decoder (`decode::cunit`) on a paused game:
//! `decoder_compare <pid>`. Pause the game first so both reads see the same bytes.

#[cfg(windows)]
fn main() {
    use std::collections::BTreeMap;

    use bwrapi_engine::decode::cunit::{
        iter_cunit_unit_facts_with_queued_orders_profile, CUNIT_ZERG_HATCHERY_FAMILY_TYPE_IDS,
    };
    use bwrapi_engine::tables::source;
    use bwrapi_engine::{unit_slot, Cell, Engine};
    use bwrapi_internal_model::adapted::ValidationStatus;
    use bwrapi_layout::{read_pointer, Layout};
    use bwrapi_memory::{ProcessId, ProcessMemory, WindowsProcessAttachment};

    let pid: u32 = std::env::args().nth(1).expect("pid").parse().expect("pid");
    let memory = WindowsProcessAttachment::attach_by_pid(ProcessId(pid)).expect("attach");
    let layout = Layout::detect(memory.main_module().size).expect("supported game build");
    let c = &layout.cunit;
    let pointer = memory.read_module_exact(c.array_pointer_rva, layout.pointer_bytes).expect("pool pointer");
    let base = read_pointer(&pointer, 0, layout.pointer_bytes).unwrap() - c.selection_ptr_adjustment;
    let capacity = u32::from_le_bytes(memory.read_module_exact(c.capacity_pointer_rva, 4).unwrap().try_into().unwrap());
    let raw = memory.read_exact(base, (capacity as usize * c.record_stride).min(c.region_bytes)).expect("pool");
    let facts = iter_cunit_unit_facts_with_queued_orders_profile(
        layout, &raw, base, None, false, ValidationStatus::Validated, true,
        |address, size| memory.read_exact(address, size).ok(),
    );

    let mut engine = Engine::open(pid).expect("open");
    engine.subscribe(source::UNITS | source::ORDERS);
    let frame = engine.acquire().expect("acquire");

    // Reference tags are "cunit:record:<offset>".
    let tag_slot = |tag: &Option<String>| -> Option<u64> {
        tag.as_deref()?.rsplit(':').next()?.parse::<usize>().ok().map(|offset| (offset / c.record_stride) as u64)
    };
    let id_slot = |id: u64| -> Option<u64> { (id != 0 && id != u64::UNKNOWN).then(|| u64::from(unit_slot(id))) };
    let flag = |value: u8| (value != u8::UNKNOWN).then_some(value != 0);
    let known = |value: u64, unknown: u64| (value != unknown).then_some(value);

    let mut mismatches: BTreeMap<&str, (usize, Vec<String>)> = BTreeMap::new();
    let mut check = |name: &'static str, slot: u64, engine: String, reference: String| {
        if engine != reference {
            let entry = mismatches.entry(name).or_default();
            entry.0 += 1;
            if entry.1.len() < 3 {
                entry.1.push(format!("slot {slot}: engine {engine} reference {reference}"));
            }
        }
    };

    let engine_rows: BTreeMap<u64, usize> =
        (0..frame.units.id.len()).map(|row| (u64::from(unit_slot(frame.units.id[row])), row)).collect();
    let mut compared = 0;
    for fact in &facts {
        let slot = (fact.record_index.unwrap() / c.record_stride) as u64;
        let Some(&row) = engine_rows.get(&slot) else {
            check("missing_in_engine", slot, "absent".into(), format!("type {:?}", fact.unit_type_id));
            continue;
        };
        compared += 1;
        let u = frame.units.row(row).unwrap();
        let order = fact.order.as_ref().unwrap();
        let production = fact.production.as_ref();
        let cunit = fact.cunit.as_ref();
        let vitals = fact.vitals.as_ref().unwrap();
        let motion = fact.motion.as_ref().unwrap();
        macro_rules! cmp {
            ($name:literal, $engine:expr, $reference:expr) => {
                check($name, slot, format!("{:?}", $engine), format!("{:?}", $reference))
            };
        }
        cmp!("type", Some(u.unit_type), fact.unit_type_id);
        cmp!("owner", Some(u.owner), fact.owner_id);
        cmp!("position", Some((i32::from(u.x), i32::from(u.y))), fact.position.map(|p| (p.x, p.y)));
        cmp!("hit_points", Some(u.hit_points), vitals.hit_points_raw);
        cmp!("energy", (u.energy != u16::UNKNOWN).then(|| i32::from(u.energy)), vitals.energy_raw);
        cmp!("order", Some(u16::from(u.order)), order.order_id);
        cmp!("order_target", id_slot(u.order_target), tag_slot(&order.target_unit_tag));
        cmp!("order_target_position", Some((i32::from(u.order_target_x), i32::from(u.order_target_y))), order.target_position.map(|p| (p.x, p.y)));
        cmp!("secondary_order", (u.secondary_order != u8::UNKNOWN).then(|| u16::from(u.secondary_order)), order.secondary_order_id);
        cmp!("completed", flag(u.completed), production.and_then(|p| p.is_completed));
        cmp!("powered", flag(u.powered), production.and_then(|p| p.is_powered));
        cmp!("burrowed", flag(u.burrowed), fact.is_burrowed);
        cmp!("cloaked", flag(u.cloaked), fact.is_cloaked);
        cmp!("in_air", flag(u.in_air), fact.in_air);
        cmp!("hallucination", flag(u.hallucination), fact.is_hallucination);
        cmp!("loaded", flag(u.loaded), fact.is_loaded);
        cmp!("transport", id_slot(u.transport), tag_slot(&fact.transport_unit_tag));
        cmp!("addon", id_slot(u.addon), production.and_then(|p| tag_slot(&p.addon_unit_tag)));
        cmp!("build_type", (u.build_type != u16::UNKNOWN).then_some(u.build_type), production.and_then(|p| p.build_type_id));
        cmp!("remaining_build_frames", known(u64::from(u.remaining_build_frames), u64::from(u32::MAX)),
            production.and_then(|p| p.remaining_build_frames).map(u64::from));
        // The hatchery larva countdown is its own column; the reference folds
        // it into remaining_train_frames, so compare it as larva_frames below.
        cmp!("remaining_train_frames", known(u64::from(u.remaining_train_frames), u64::from(u32::MAX)),
            production
                .filter(|_| !CUNIT_ZERG_HATCHERY_FAMILY_TYPE_IDS.contains(&u.unit_type))
                .and_then(|p| p.remaining_train_frames)
                .map(u64::from));
        let mut reference_queue: Vec<u16> =
            production.and_then(|p| p.training_queue.clone()).unwrap_or_default();
        reference_queue.resize(5, u16::UNKNOWN);
        cmp!("train_queue",
            [u.train_queue_0, u.train_queue_1, u.train_queue_2, u.train_queue_3, u.train_queue_4],
            &reference_queue[..]);
        cmp!("tech", (u.tech != u8::UNKNOWN).then(|| u16::from(u.tech)), production.and_then(|p| p.tech_type_id));
        cmp!("remaining_research_frames", known(u64::from(u.remaining_research_frames), u64::from(u32::MAX)),
            production.and_then(|p| p.remaining_research_frames).map(u64::from));
        cmp!("upgrade", (u.upgrade != u8::UNKNOWN).then(|| u16::from(u.upgrade)), production.and_then(|p| p.upgrade_type_id));
        cmp!("remaining_upgrade_frames", known(u64::from(u.remaining_upgrade_frames), u64::from(u32::MAX)),
            production.and_then(|p| p.remaining_upgrade_frames).map(u64::from));
        cmp!("build_unit", id_slot(u.build_unit), production.and_then(|p| tag_slot(&p.build_unit_tag)));
        cmp!("move_target", id_slot(u.move_target), tag_slot(&order.move_target_unit_tag));
        cmp!("move_target_position", Some((i32::from(u.move_target_x), i32::from(u.move_target_y))),
            order.move_target_position.map(|p| (p.x, p.y)));
        cmp!("sub_unit", id_slot(u.sub_unit), cunit.and_then(|x| tag_slot(&x.sub_unit_tag)));
        cmp!("carrier", id_slot(u.carrier), cunit.and_then(|x| tag_slot(&x.hangar_parent_unit_tag)));
        cmp!("nydus_exit", id_slot(u.nydus_exit), cunit.and_then(|x| tag_slot(&x.nydus_exit_unit_tag)));
        cmp!("power_up", id_slot(u.power_up), cunit.and_then(|x| tag_slot(&x.power_up_unit_tag)));
        cmp!("interruptible", flag(u.interruptible), cunit.and_then(|x| x.is_interruptible));
        cmp!("hatchery", id_slot(u.hatchery), cunit.and_then(|x| tag_slot(&x.hatchery_unit_tag)));
        cmp!("spider_mines", (u.spider_mines != u8::UNKNOWN).then_some(u.spider_mines), cunit.and_then(|x| x.spider_mine_count));
        cmp!("remove_timer", Some(u.remove_timer), cunit.and_then(|x| x.remove_timer));
        cmp!("timers", Some([u.lockdown_timer, u.maelstrom_timer, u.stasis_timer, u.irradiate_timer, u.plague_timer, u.ensnare_timer, u.defense_matrix_timer]),
            cunit.map(|x| [x.lockdown_timer, x.maelstrom_timer, x.stasis_timer, x.irradiate_timer, x.plague_timer, x.ensnare_timer, x.defense_matrix_timer].map(|t| t.unwrap_or(u8::MAX))));
        cmp!("rally_position", (u.rally_x != i16::UNKNOWN).then(|| (i32::from(u.rally_x), i32::from(u.rally_y))),
            order.rally_position.map(|p| (p.x, p.y)));
        cmp!("rally_unit", id_slot(u.rally_unit), tag_slot(&order.rally_unit_tag));
        if fact.unit_type_id.is_some_and(|unit_type| CUNIT_ZERG_HATCHERY_FAMILY_TYPE_IDS.contains(&unit_type)) {
            cmp!("larva_frames", known(u64::from(u.larva_frames), u64::from(u32::MAX)),
                production.and_then(|p| p.remaining_train_frames).map(u64::from));
        }
        cmp!("order_timer", Some(u.order_timer), order.order_timer);
        cmp!("stim_timer", Some(u.stim_timer), cunit.and_then(|x| x.stim_timer));
        cmp!("storm_timer", Some(u.storm_timer), cunit.and_then(|x| x.storm_timer));
        cmp!("defense_matrix_damage", Some(u.defense_matrix_damage), cunit.and_then(|x| x.defense_matrix_damage_raw));
        cmp!("cooldowns", Some([u.ground_cooldown, u.air_cooldown, u.spell_cooldown]),
            cunit.map(|x| [x.ground_weapon_cooldown, x.air_weapon_cooldown, x.spell_cooldown].map(|t| t.unwrap_or(u8::MAX))));
        cmp!("kill_count", Some(u.kill_count), cunit.and_then(|x| x.kill_count));
        cmp!("acid_spores", Some(u.acid_spores), cunit.and_then(|x| x.acid_spore_count));
        cmp!("scarabs", (u.scarabs != u8::UNKNOWN).then_some(u.scarabs), cunit.and_then(|x| x.scarab_count));
        cmp!("interceptors", (u.interceptors != u8::UNKNOWN).then_some(u.interceptors), cunit.and_then(|x| x.interceptor_count));
        cmp!("resource_group", (u.resource_group != u8::UNKNOWN).then_some(u.resource_group), cunit.and_then(|x| x.resource_group));
        // The reference cargo kind omits a known empty cargo; the column keeps 0.
        cmp!("carry", (u.carry != u8::UNKNOWN).then_some(u.carry).filter(|kind| *kind != 0),
            cunit.and_then(|x| x.resource_cargo_kind));
        cmp!("last_attacker", (u.last_attacker != u8::UNKNOWN).then_some(u.last_attacker), cunit.and_then(|x| x.last_attacking_player_id));
        cmp!("recently_attacked", flag(u.recently_attacked), cunit.and_then(|x| x.recently_attacked));
        cmp!("button_set", Some(u.button_set), cunit.and_then(|x| x.button_set_id));
        cmp!("previous_hit_points", Some(u.previous_hit_points), cunit.and_then(|x| x.previous_hit_points_raw));
        cmp!("being_healed", flag(u.being_healed), cunit.and_then(|x| x.is_being_healed));
        cmp!("gathering", flag(u.gathering), cunit.and_then(|x| x.is_gathering));
        cmp!("blind", flag(u.blind), cunit.and_then(|x| x.is_blind));
        cmp!("parasited", flag(u.parasited), cunit.and_then(|x| x.is_parasited));
        cmp!("has_nuke", flag(u.has_nuke), cunit.and_then(|x| x.has_nuke));
        cmp!("stuck", flag(u.stuck), cunit.and_then(|x| x.is_stuck));
        cmp!("moving", flag(u.moving), motion.is_moving);
        cmp!("accelerating", flag(u.accelerating), motion.is_accelerating);
        cmp!("braking", flag(u.braking), motion.is_braking);
        cmp!("facing", Some(u.facing), motion.facing_direction_raw);
        cmp!("velocity", Some((u.velocity_x, u.velocity_y)),
            motion.velocity_x_raw.zip(motion.velocity_y_raw));

        let engine_queue: Vec<_> = (0..frame.orders.unit.len())
            .filter(|row| frame.orders.unit[*row] == u.id)
            .map(|row| (u16::from(frame.orders.order[row]), frame.orders.x[row] as i32, frame.orders.y[row] as i32, id_slot(frame.orders.target[row])))
            .collect();
        let reference_queue: Vec<_> = order.queued_orders.iter()
            .map(|q| (q.order_id.unwrap_or(u16::MAX), q.target_position.map_or(0, |p| p.x), q.target_position.map_or(0, |p| p.y), tag_slot(&q.target_unit_tag)))
            .collect();
        cmp!("queue", engine_queue, reference_queue);
    }
    let reference_slots: std::collections::BTreeSet<u64> =
        facts.iter().map(|fact| (fact.record_index.unwrap() / c.record_stride) as u64).collect();
    for slot in engine_rows.keys().filter(|slot| !reference_slots.contains(slot)) {
        check("missing_in_reference", *slot, "present".into(), "absent".into());
    }

    println!("frame {} engine units {} reference units {} compared {}", frame.game_frame, frame.units.id.len(), facts.len(), compared);
    if mismatches.is_empty() {
        println!("all columns match");
    }
    for (name, (count, examples)) in mismatches {
        println!("{name}: {count} mismatches");
        for example in examples {
            println!("  {example}");
        }
    }
}

#[cfg(not(windows))]
fn main() {}
