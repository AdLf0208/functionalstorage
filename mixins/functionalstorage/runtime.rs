// Functional Storage for Horizon — the storage-drawer engine (pure).
//
// Ported from Functional Storage 1.5.8 (`Buuz135/FunctionalStorage`, MIT),
// Minecraft Java 1.21.1 NeoForge. The upstream source was read for behaviour
// only; this is a clean-room Rust rewrite.
//
// Slice 1: the 1x1 storage drawers (`<wood>_1`). A drawer stores one item
// kind in one big slot. Upstream's capacity is `64 * base`, where a 1x1
// drawer's base is 32, scaled by the item's own stack size
// (`BigInventoryHandler.getSlotLimit`: `floor(total * maxStack / 64)`), so a
// 64-stack item holds 2048 and a 16-stack item 512. The item kind is locked
// by the first item inserted; a locked empty drawer refuses new kinds.
//
// The pin's container slot is an `ItemStack` whose count is at most the
// item's stack size, while a drawer holds far more. The engine therefore
// keeps the true amount and exposes a *view*: the slot carries
// `ceil(amount * maxStack / limit)` (1..=maxStack) of the stored item. Every
// external change to that slot (a hopper, a menu take) is reconciled against
// the last view at the block entity's tick. This is the port's documented
// representation, not upstream's `IItemHandler` with a count of 2048.

/// The wood-coloured 1x1 drawers carried by this slice.
pub const DRAWER_BLOCKS: &[&str] = &[
    "functionalstorage:oak_1",
    "functionalstorage:spruce_1",
    "functionalstorage:birch_1",
    "functionalstorage:jungle_1",
    "functionalstorage:acacia_1",
    "functionalstorage:dark_oak_1",
    "functionalstorage:crimson_1",
    "functionalstorage:warped_1",
    "functionalstorage:mangrove_1",
    "functionalstorage:cherry_1",
];

/// `DrawerType.X_1.getSlotAmount()`: the 1x1 drawer's base multiplier.
pub const BASE_MULTIPLIER: u32 = 32;

/// Whether `block` is a drawer this slice implements.
pub fn is_drawer(block: &str) -> bool {
    DRAWER_BLOCKS.contains(&block)
}

/// The capacity of one slot for an item whose stack size is `max_stack`:
/// upstream's `floor(64 * base * maxStack / 64)`.
pub fn capacity(max_stack: u32) -> u32 {
    BASE_MULTIPLIER * max_stack.max(1)
}

/// One big slot: the item kind (an identity key, item id plus its components)
/// and the true amount stored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BigSlot {
    pub item: Option<String>,
    pub amount: u32,
}

/// A drawer's stored state: its big slots and whether it is locked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Drawer {
    pub slots: Vec<BigSlot>,
    pub locked: bool,
}

/// What an external change to the view slot was, and what to hand back when
/// the drawer refused it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sync {
    /// An item stack the drawer would not keep: the caller returns it to the
    /// player or drops it.
    pub rejected: Option<(String, u32)>,
    /// Whether the stored amount changed (so the comparator may need an
    /// update).
    pub changed: bool,
}

impl Drawer {
    pub fn new(slot_count: usize) -> Self {
        Self {
            slots: vec![BigSlot::default(); slot_count],
            locked: false,
        }
    }

    pub fn amount(&self, slot: usize) -> u32 {
        self.slots.get(slot).map_or(0, |s| s.amount)
    }

    pub fn item(&self, slot: usize) -> Option<&str> {
        self.slots.get(slot).and_then(|s| s.item.as_deref())
    }

    /// The item's limit in `slot`, or 0 for a slot that does not exist.
    pub fn limit(&self, slot: usize, max_stack: u32) -> u32 {
        if slot >= self.slots.len() {
            0
        } else {
            capacity(max_stack)
        }
    }

    /// How many of `key` fit in `slot` right now: 0 when the drawer is locked
    /// empty or holds another kind.
    pub fn room(&self, slot: usize, key: &str, max_stack: u32) -> u32 {
        let Some(big) = self.slots.get(slot) else {
            return 0;
        };
        match big.item.as_deref() {
            Some(held) if held != key => 0,
            Some(_) => self.limit(slot, max_stack).saturating_sub(big.amount),
            None if self.locked => 0,
            None => self.limit(slot, max_stack),
        }
    }

    /// Inserts up to `count` of `key`; returns how many were taken.
    pub fn insert(&mut self, slot: usize, key: &str, count: u32, max_stack: u32) -> u32 {
        let room = self.room(slot, key, max_stack);
        let taken = room.min(count);
        if taken == 0 {
            return 0;
        }
        let Some(big) = self.slots.get_mut(slot) else {
            return 0;
        };
        if big.item.is_none() {
            big.item = Some(key.to_owned());
        }
        big.amount += taken;
        taken
    }

    /// Takes up to `count` out of `slot`; returns how many were taken.
    #[allow(dead_code)]
    pub fn extract(&mut self, slot: usize, count: u32, max_stack: u32) -> u32 {
        let limit = self.limit(slot, max_stack);
        let Some(big) = self.slots.get_mut(slot) else {
            return 0;
        };
        let taken = big.amount.min(count).min(limit);
        big.amount -= taken;
        if big.amount == 0 {
            big.item = None;
        }
        taken
    }

    /// The count the container slot shows: `ceil(amount * maxStack / limit)`,
    /// clamped to `1..=maxStack`, or 0 when empty.
    pub fn view_count(&self, slot: usize, max_stack: u32) -> u32 {
        let amount = self.amount(slot);
        if amount == 0 {
            return 0;
        }
        let limit = self.limit(slot, max_stack).max(1);
        let max = max_stack.max(1);
        let scaled = (u64::from(amount) * u64::from(max)).div_ceil(u64::from(limit));
        (scaled as u32).clamp(1, max)
    }

    /// Reconciles the container slot after an external change. `observed` is
    /// the slot's stack now (`key`, count) or `None` when it is empty; the
    /// last view the drawer wrote is its current `view_count`.
    ///
    /// A take lowers the amount by the old view; a matching insert raises it
    /// by the difference. A different kind (a swap the menu should not allow)
    /// is refused and returned in `rejected`.
    pub fn sync_view(
        &mut self,
        slot: usize,
        observed: Option<(&str, u32)>,
        max_stack: u32,
    ) -> Sync {
        let view = self.view_count(slot, max_stack);
        let limit = self.limit(slot, max_stack);
        let locked = self.locked;
        let Some(big) = self.slots.get_mut(slot) else {
            return Sync::default();
        };
        let before = big.amount;
        match (big.item.clone(), observed) {
            (None, None) => {}
            (None, Some((key, count))) => {
                if locked {
                    return Sync {
                        rejected: Some((key.to_owned(), count)),
                        changed: false,
                    };
                }
                big.item = Some(key.to_owned());
                big.amount = count.min(limit);
            }
            (Some(_held), None) => {
                big.amount = big.amount.saturating_sub(view);
                if big.amount == 0 {
                    big.item = None;
                }
            }
            (Some(held), Some((key, count))) => {
                if key == held {
                    let adjusted = big.amount as i64 - i64::from(view) + i64::from(count);
                    big.amount = adjusted.clamp(0, i64::from(limit)) as u32;
                    if big.amount == 0 {
                        big.item = None;
                    }
                } else {
                    // A swap: the old view left and another kind arrived. The
                    // menu's result slot makes this unreachable for players;
                    // a pipe that bypassed the same-item check gets the stack
                    // back.
                    big.amount = big.amount.saturating_sub(view);
                    if big.amount == 0 {
                        big.item = None;
                        return Sync {
                            rejected: Some((key.to_owned(), count)),
                            changed: before != 0,
                        };
                    }
                    return Sync {
                        rejected: Some((key.to_owned(), count)),
                        changed: false,
                    };
                }
            }
        }
        Sync {
            rejected: None,
            changed: big.amount != before,
        }
    }

    /// `DrawerBlock.getAnalogOutputSignal`: the average fullness over the
    /// slots, `floor(fullness * 14) + 1`, 0 when empty.
    pub fn signal(&self, max_stack: u32) -> i32 {
        if self.slots.is_empty() {
            return 0;
        }
        let mut fullness = 0.0f64;
        let mut has_contents = false;
        for (index, big) in self.slots.iter().enumerate() {
            if big.amount > 0 {
                has_contents = true;
                let limit = self.limit(index, max_stack).max(1);
                fullness += f64::from(big.amount.min(limit)) / f64::from(limit);
            }
        }
        if !has_contents {
            return 0;
        }
        let average = fullness / self.slots.len() as f64;
        ((average * 14.0).floor() as i32 + 1).min(15)
    }

    /// Sneak-right-click with an empty hand toggles the lock; returns the new
    /// state.
    pub fn toggle_locked(&mut self) -> bool {
        self.locked = !self.locked;
        self.locked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawer() -> Drawer {
        Drawer::new(1)
    }

    #[test]
    fn a_1x1_drawer_holds_2048_of_a_64_stack_item() {
        assert_eq!(capacity(64), 2048);
        assert_eq!(capacity(16), 512);
        assert_eq!(capacity(1), 32);
        assert_eq!(drawer().limit(0, 64), 2048);
        assert_eq!(drawer().limit(1, 64), 0);
    }

    #[test]
    fn the_first_item_kinds_the_drawer_and_others_are_refused() {
        let mut d = drawer();
        assert_eq!(d.insert(0, "minecraft:stone", 100, 64), 100);
        assert_eq!(d.insert(0, "minecraft:dirt", 10, 64), 0);
        assert_eq!(d.insert(0, "minecraft:stone", 10, 64), 10);
        assert_eq!(d.amount(0), 110);
        assert_eq!(d.item(0), Some("minecraft:stone"));
    }

    #[test]
    fn insertion_is_capped_by_the_limit_and_returns_the_rest() {
        let mut d = drawer();
        assert_eq!(d.insert(0, "minecraft:stone", 3000, 64), 2048);
        assert_eq!(d.insert(0, "minecraft:stone", 1, 64), 0);
        assert_eq!(d.amount(0), 2048);
        // A 16-stack item has a quarter of the capacity.
        let mut d = drawer();
        assert_eq!(d.insert(0, "minecraft:snowball", 600, 16), 512);
    }

    #[test]
    fn a_locked_empty_drawer_refuses_new_items_but_a_locked_full_one_keeps_its_kind() {
        let mut d = drawer();
        d.toggle_locked();
        assert!(d.locked);
        assert_eq!(d.insert(0, "minecraft:stone", 5, 64), 0);
        d.toggle_locked();
        assert_eq!(d.insert(0, "minecraft:stone", 5, 64), 5);
        d.toggle_locked();
        assert_eq!(d.insert(0, "minecraft:stone", 5, 64), 5);
        assert_eq!(d.insert(0, "minecraft:dirt", 5, 64), 0);
        assert_eq!(d.amount(0), 10);
    }

    #[test]
    fn extraction_takes_what_is_asked_and_clears_the_kind_at_zero() {
        let mut d = drawer();
        d.insert(0, "minecraft:stone", 10, 64);
        assert_eq!(d.extract(0, 4, 64), 4);
        assert_eq!(d.amount(0), 6);
        assert_eq!(d.extract(0, 64, 64), 6);
        assert_eq!(d.amount(0), 0);
        assert_eq!(d.item(0), None);
        assert_eq!(d.extract(0, 1, 64), 0);
    }

    #[test]
    fn the_view_slot_shows_a_proportional_count_at_most_a_stack() {
        let mut d = drawer();
        assert_eq!(d.view_count(0, 64), 0);
        d.insert(0, "minecraft:stone", 1, 64);
        assert_eq!(d.view_count(0, 64), 1);
        d.insert(0, "minecraft:stone", 1023, 64); // 1024/2048
        assert_eq!(d.view_count(0, 64), 32);
        d.insert(0, "minecraft:stone", 1024, 64); // full
        assert_eq!(d.view_count(0, 64), 64);
        let mut d = drawer();
        d.insert(0, "minecraft:snowball", 256, 16); // half of 512
        assert_eq!(d.view_count(0, 16), 8);
    }

    #[test]
    fn a_take_through_the_view_lowers_the_true_amount() {
        let mut d = drawer();
        d.insert(0, "minecraft:stone", 100, 64);
        let view = d.view_count(0, 64); // ceil(100*64/2048) = 4
        assert_eq!(view, 4);
        let sync = d.sync_view(0, None, 64);
        assert!(sync.changed);
        assert_eq!(sync.rejected, None);
        assert_eq!(d.amount(0), 96);
    }

    #[test]
    fn a_matching_insert_through_the_view_raises_the_amount_by_the_delta() {
        let mut d = drawer();
        d.insert(0, "minecraft:stone", 100, 64); // view 4
        let sync = d.sync_view(0, Some(("minecraft:stone", 10)), 64);
        assert!(sync.changed);
        assert_eq!(d.amount(0), 106);
    }

    #[test]
    fn a_hopper_take_of_one_from_a_large_drawer_lowers_it_by_one() {
        let mut d = drawer();
        d.insert(0, "minecraft:stone", 2000, 64); // view 63
        let sync = d.sync_view(0, Some(("minecraft:stone", 62)), 64);
        assert_eq!(sync.rejected, None);
        assert_eq!(d.amount(0), 1999);
    }

    #[test]
    fn an_empty_slot_adopts_the_first_hopper_item() {
        let mut d = drawer();
        let sync = d.sync_view(0, Some(("minecraft:dirt", 7)), 64);
        assert_eq!(sync.rejected, None);
        assert_eq!(d.item(0), Some("minecraft:dirt"));
        assert_eq!(d.amount(0), 7);
    }

    #[test]
    fn a_locked_empty_drawer_hands_a_hopper_item_back() {
        let mut d = drawer();
        d.toggle_locked();
        let sync = d.sync_view(0, Some(("minecraft:dirt", 7)), 64);
        assert_eq!(sync.rejected, Some(("minecraft:dirt".to_owned(), 7)));
        assert_eq!(d.amount(0), 0);
        assert_eq!(d.item(0), None);
    }

    #[test]
    fn a_swap_keeps_the_bulk_and_hands_the_other_kind_back() {
        let mut d = drawer();
        d.insert(0, "minecraft:stone", 100, 64); // view 4
        let sync = d.sync_view(0, Some(("minecraft:dirt", 3)), 64);
        assert_eq!(sync.rejected, Some(("minecraft:dirt".to_owned(), 3)));
        assert_eq!(d.item(0), Some("minecraft:stone"));
        assert_eq!(d.amount(0), 96);
    }

    #[test]
    fn the_comparator_signal_follows_fullness() {
        let mut d = drawer();
        assert_eq!(d.signal(64), 0);
        d.insert(0, "minecraft:stone", 1, 64);
        assert_eq!(d.signal(64), 1);
        d.insert(0, "minecraft:stone", 1023, 64); // half
        assert_eq!(d.signal(64), 8);
        d.insert(0, "minecraft:stone", 1024, 64); // full
        assert_eq!(d.signal(64), 15);
    }

    #[test]
    fn the_lock_toggles() {
        let mut d = drawer();
        assert!(!d.locked);
        assert!(d.toggle_locked());
        assert!(!d.toggle_locked());
    }

    #[test]
    fn a_take_is_not_undone_by_the_same_ticks_view_sync() {
        let mut d = drawer();
        d.insert(0, "minecraft:stone", 64, 64);
        // The view the slot held before our own take.
        let old_view = d.view_count(0, 64);
        assert_eq!(old_view, 2);
        // 1. the external sync runs first.
        let sync = d.sync_view(0, Some(("minecraft:stone", old_view)), 64);
        assert_eq!(sync.rejected, None);
        // 2. the queued take decrements exactly once.
        assert_eq!(d.extract(0, 1, 64), 1);
        assert_eq!(d.amount(0), 63);
        // 3. the next tick's fresh view does not restore the old amount.
        let next_view = d.view_count(0, 64);
        let sync = d.sync_view(0, Some(("minecraft:stone", next_view)), 64);
        assert!(!sync.changed);
        assert_eq!(d.amount(0), 63);
    }

    #[test]
    fn taking_the_last_item_leaves_the_slot_empty() {
        let mut d = drawer();
        d.insert(0, "minecraft:stone", 1, 64);
        assert_eq!(d.view_count(0, 64), 1);
        assert_eq!(d.extract(0, 1, 64), 1);
        assert_eq!(d.amount(0), 0);
        assert_eq!(d.item(0), None);
        assert_eq!(d.view_count(0, 64), 0);
        let sync = d.sync_view(0, None, 64);
        assert!(!sync.changed);
        assert_eq!(d.amount(0), 0);
    }

    #[test]
    fn a_take_of_a_stack_keeps_the_true_amount() {
        let mut d = drawer();
        d.insert(0, "minecraft:stone", 100, 64);
        assert_eq!(d.extract(0, 64, 64), 64);
        assert_eq!(d.amount(0), 36);
        d.insert(0, "minecraft:stone", 64, 64);
        assert_eq!(d.amount(0), 100);
        assert_eq!(d.extract(0, 64, 64), 64);
        assert_eq!(d.amount(0), 36);
    }

    #[test]
    fn every_carried_block_is_a_drawer() {
        assert!(is_drawer("functionalstorage:oak_1"));
        assert!(is_drawer("functionalstorage:cherry_1"));
        assert!(!is_drawer("functionalstorage:oak_2"));
        assert!(!is_drawer("minecraft:barrel"));
        assert_eq!(DRAWER_BLOCKS.len(), 10);
    }
}
