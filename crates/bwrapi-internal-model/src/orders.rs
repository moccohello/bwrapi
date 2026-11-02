pub const ORDER_COUNT: usize = 192;

/// BWAPI 4.4 UnitUpdate suppresses Completed while a Zerg morph order runs.
pub fn is_bwapi_completed(raw_completed: bool, order_id: u16) -> bool {
    raw_completed && !matches!(order_id, 41 | 42 | 43 | 45)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_tracks_morph_start_cancel_and_finish() {
        for order in [41, 42, 43, 45] {
            assert!(!is_bwapi_completed(true, order));
            assert!(!is_bwapi_completed(false, order));
        }
        // Cancel/finish returns to the idle order; unfinished construction
        // must still wait for the raw completion bit.
        assert!(is_bwapi_completed(true, 1));
        assert!(!is_bwapi_completed(false, 1));
        assert!(is_bwapi_completed(true, 44));
    }
}
