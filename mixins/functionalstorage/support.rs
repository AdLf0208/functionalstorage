// Game-side glue for Functional Storage slice 1 (the 1x1 storage drawers).
//
// Injected into `crate::game` as `functionalstorage_state`; it needs the game,
// so it is not part of the standalone library (only `runtime.rs` is).
//
// The pin's container slot is one `ItemStack` (at most a stack), while a
// drawer holds up to 2048. The block entity's saved compound keeps the true
// amount and the stored stack (components included); the container slot is a
// proportional view of it (`runtime::Drawer::view_count`). Every external
// change of that slot (a hopper, a menu take) is reconciled in the block
// entity's tick (`runtime::Drawer::sync_view`).
//
// A broken drawer's contents ride its item component
// (`functionalstorage:drawer`): `destroy_block`/`explosion_detonate` read the
// block entity before it goes and empty the view slot (so the container's
// removal spills nothing), `loot_drops` attaches the saved compound to the
// drawer item drop, and `place_block` restores it.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use horizon_assets::item_model::DisplayContext;
use horizon_client::containers::{BlockEntity, ModBlockEntity, ModContainer};
use horizon_client::hud::HudBatch;
use horizon_client::items::ItemStack;
use crate::game::native_hooks::NativeScreen;
use horizon_client::save::{item_from_nbt, item_to_nbt};
use horizon_client::text::Text;
use horizon_world::nbt::{Compound, JavaString, Tag};
use horizon_world::registry::BlockStateId;

use crate::game::functionalstorage::{Drawer, capacity, is_drawer};
use crate::game::native_block_entity_draw::BlockEntityDraw;
use crate::game::native_hooks::NativeLootDrops;
use crate::game::native_lifecycle::ServerTickPhase;
use crate::game::GameLevel;
use crate::game::native_server::{NativeCommand, NativeServerPlayer};
use crate::game::Game;

/// The menu type the drawer's content mod declares (`data/<id>/horizon/menu/`).
pub const DRAWER_MENU: &str = "functionalstorage:drawer";
/// The item component carrying a broken drawer's contents.
pub const DRAWER_COMPONENT: &str = "functionalstorage:drawer";
/// The block entity's saved keys.
const KEY_ITEM: &str = "item";
const KEY_AMOUNT: &str = "amount";
const KEY_LOCKED: &str = "locked";
const KEY_LIMIT: &str = "limit";

/// A broken drawer's contents, waiting for its loot drop.
struct Stash {
    dimension: String,
    pos: [i32; 3],
    data: Compound,
    at: Instant,
}


/// A take that did not fit, dropped at the next level tick.

#[derive(Default)]
struct State {
    stash: Vec<Stash>,
    /// The acting player's held stack changed; a guest needs an explicit sync.
    sync: Vec<u128>,
    /// A drawer's comparator answer changed; refresh its output at tick end.
    output: Vec<(String, [i32; 3])>,
    /// The last whole-inventory insert attempt of each player (300 ms).
    last_click: HashMap<u128, Instant>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn shared_state() -> std::sync::MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(|poison| poison.into_inner())
}

// ---------------------------------------------------------------------------
// Declaration hooks
// ---------------------------------------------------------------------------

/// Give each drawer a block entity (`native_block_entity`, API 15).
#[allow(private_interfaces)]
pub fn on_block_entity(block: &str, entity: &mut Option<(String, Compound)>) {
    if is_drawer(block) {
        *entity = Some((block.to_owned(), Compound::new()));
    }
}

/// The drawers tick (`native_block_entity_ticks`, API 15).
#[allow(private_interfaces)]
pub fn on_block_entity_ticks(block: &str, ticks: &mut bool) {
    if is_drawer(block) {
        *ticks = true;
    }
}

/// One big slot behind the drawer, and the drawer's one-slot menu
/// (`native_block_entity_container`, API 16/17). Every face reaches the slot;
/// hoppers may both fill and empty it (the view).
#[allow(private_interfaces)]
pub fn on_container(block: &str, container: &mut Option<ModContainer>) {
    if !is_drawer(block) {
        return;
    }
    *container = Some(ModContainer {
        slots: 1,
        faces: std::array::from_fn(|_| vec![0]),
        inputs: vec![0],
        outputs: vec![0],
        menu: Some(DRAWER_MENU.to_owned()),
    });
}

/// A placed drawer faces its placer (`native_state_for_placement`, API 76),
/// as upstream's `getNearestLookingDirection().getOpposite()` does.
#[allow(private_interfaces)]
pub fn on_state_for_placement(
    level: &crate::game::GameLevel<'_>,
    state: BlockStateId,
    context: &horizon_world::level::PlacementContext,
    answer: &mut Option<Option<BlockStateId>>,
) {
    let Some(block) = level.registry.state(state).map(|state| state.name.as_str()) else {
        return;
    };
    if !is_drawer(block) {
        return;
    }
    if let Some(facing) =
        level.native_state_with_property(state, "facing", facing_for_yaw(context.yaw))
    {
        *answer = Some(Some(facing));
    }
}

/// The horizontal `facing` a drawer takes when a placer with `yaw` places it:
/// the direction toward the placer (its front faces the player).
#[allow(private_interfaces)]
pub fn facing_for_yaw(yaw: f32) -> &'static str {
    let quarter = ((f64::from(yaw) / 90.0 + 0.5).floor() as i64 & 3) as u8;
    match quarter {
        0 => "north",
        1 => "east",
        2 => "south",
        _ => "west",
    }
}

/// The block face a `facing` property names, as a unit offset.
#[allow(private_interfaces)]
pub fn facing_offset(facing: &str) -> [i32; 3] {
    match facing {
        "north" => [0, 0, -1],
        "south" => [0, 0, 1],
        "west" => [-1, 0, 0],
        "east" => [1, 0, 0],
        _ => [0, 0, -1],
    }
}

/// The yaw the front model turns through for a `facing` property.
fn facing_yaw(facing: &str) -> f32 {
    match facing {
        "east" => 90.0,
        "south" => 180.0,
        "west" => 270.0,
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------
// Storage helpers
// ---------------------------------------------------------------------------

/// The identity key of a stack: its item and every component, as the pure
/// engine compares them.
fn stack_key(stack: &ItemStack) -> String {
    format!("{}|{}|{:?}", stack.item, stack.damage, stack.extra)
}

fn max_stack_of(game: &Game<'_>, item: &str) -> u32 {
    game.items()
        .get(item)
        .map_or(64, |info| info.max_stack)
        .max(1)
}

fn stored_from_nbt(nbt: &Compound) -> Option<ItemStack> {
    item_from_nbt(nbt.get(&JavaString::from(KEY_ITEM))?)
}

fn read_drawer(nbt: &Compound) -> (Drawer, Option<ItemStack>) {
    let mut drawer = Drawer::new(1);
    drawer.locked = matches!(nbt.get(&JavaString::from(KEY_LOCKED)), Some(Tag::Byte(1)));
    let stored = stored_from_nbt(nbt);
    if let Some(stack) = &stored {
        let amount = match nbt.get(&JavaString::from(KEY_AMOUNT)) {
            Some(Tag::Int(n)) if *n > 0 => *n as u32,
            _ => 0,
        };
        if amount > 0 {
            drawer.slots[0].item = Some(stack_key(stack));
            drawer.slots[0].amount = amount;
        }
    }
    (drawer, stored)
}

fn write_drawer(nbt: &mut Compound, drawer: &Drawer, stored: Option<&ItemStack>, limit: u32) {
    if let (Some(stack), true) = (stored, drawer.amount(0) > 0) {
        let mut one = stack.clone();
        one.count = 1;
        nbt.insert(JavaString::from(KEY_ITEM), item_to_nbt(&one));
        nbt.insert(
            JavaString::from(KEY_AMOUNT),
            Tag::Int(drawer.amount(0) as i32),
        );
    } else {
        nbt.remove(&JavaString::from(KEY_ITEM));
        nbt.remove(&JavaString::from(KEY_AMOUNT));
    }
    nbt.insert(
        JavaString::from(KEY_LOCKED),
        Tag::Byte(if drawer.locked { 1 } else { 0 }),
    );
    nbt.insert(JavaString::from(KEY_LIMIT), Tag::Int(limit as i32));
}

/// The comparator answer for a drawer whose limit is `limit`.
fn drawer_signal(drawer: &Drawer, limit: u32) -> i32 {
    let amount = drawer.amount(0);
    if amount == 0 {
        return 0;
    }
    let limit = limit.max(1);
    let fullness = f64::from(amount.min(limit)) / f64::from(limit);
    ((fullness * 14.0).floor() as i32 + 1).min(15)
}

/// The stack the container slot shows for `drawer` (`view_count` of the
/// stored stack, components kept).
fn view_stack(drawer: &Drawer, stored: Option<&ItemStack>, max_stack: u32) -> Option<ItemStack> {
    let view = drawer.view_count(0, max_stack);
    let mut stack = stored?.clone();
    if view == 0 {
        return None;
    }
    stack.count = view;
    Some(stack)
}

fn signal_of(game: &Game<'_>, drawer: &Drawer, stored: Option<&ItemStack>) -> i32 {
    let max_stack = stored.map_or(64, |stack| max_stack_of(game, &stack.item));
    drawer.signal(max_stack)
}

/// Writes a drawer's state back to its block entity, its view slot and its
/// menu data slots. `old_signal` is the comparator answer before the change.
fn write_entity(
    game: &mut Game<'_>,
    pos: [i32; 3],
    drawer: &Drawer,
    stored: Option<&ItemStack>,
    old_signal: i32,
) {
    let max_stack = stored.map_or(64, |stack| max_stack_of(game, &stack.item));
    let view = view_stack(drawer, stored, max_stack);
    let data = vec![
        drawer.amount(0) as i32,
        drawer.limit(0, max_stack) as i32,
        if drawer.locked { 1 } else { 0 },
    ];
    let mut item_changed = false;
    let _ = game.native_with_mod_block_entity(pos, |entity| {
        let before = entity.nbt.get(&JavaString::from(KEY_ITEM)).cloned();
        write_drawer(&mut entity.nbt, drawer, stored, max_stack);
        item_changed = before != entity.nbt.get(&JavaString::from(KEY_ITEM)).cloned();
        if entity.items.is_empty() {
            entity.items.resize(1, None);
        }
        entity.items[0] = view.clone();
        if entity.data != data {
            entity.data = data;
        }
    });
    if item_changed {
        game.native_block_entity_sync(pos);
    }
    if drawer.signal(max_stack) != old_signal {
        let dimension = game.native_dimension().to_owned();
        let mut guard = shared_state();
        let state = guard.get_or_insert_with(State::default);
        if !state
            .output
            .iter()
            .any(|(d, p)| *d == dimension && *p == pos)
        {
            state.output.push((dimension, pos));
        }
    }
}

/// Reads the drawer at `pos` and applies `change`; writes it back. `change`
/// returns a new stored stack when the drawer adopted one.
fn mutate(
    game: &mut Game<'_>,
    pos: [i32; 3],
    change: impl FnOnce(&mut Drawer, Option<&ItemStack>) -> Option<ItemStack>,
) -> bool {
    let Some((mut drawer, stored)) = read_state(game, pos) else {
        return false;
    };
    let old_signal = signal_of(game, &drawer, stored.as_ref());
    let new_stored = change(&mut drawer, stored.as_ref()).or_else(|| {
        if drawer.item(0).is_some() {
            stored.clone()
        } else {
            None
        }
    });
    write_entity(game, pos, &drawer, new_stored.as_ref(), old_signal);
    true
}

fn read_state(game: &Game<'_>, pos: [i32; 3]) -> Option<(Drawer, Option<ItemStack>)> {
    match game.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
        Some(BlockEntity::Mod(entity)) if is_drawer(&entity.id) => Some(read_drawer(&entity.nbt)),
        _ => None,
    }
}

/// Drops `stack` as an item entity at the block's centre (what
/// `Block.popResource` does for a drawer's own drop).
/// Drops `stack` at the block's centre (a rejected sync stack).
fn eject(game: &mut Game<'_>, pos: [i32; 3], stack: ItemStack) {
    if stack.count == 0 {
        return;
    }
    let mut tag = Compound::new();
    tag.insert(JavaString::from("Item"), item_to_nbt(&stack));
    tag.insert(JavaString::from("PickupDelay"), Tag::Short(10));
    let dimension = game.native_dimension().to_owned();
    game.native_summon(
        &dimension,
        "minecraft:item",
        [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]) + 0.5,
            f64::from(pos[2]) + 0.5,
        ],
        tag,
    );
}

/// Queues the acting player's inventory for a guest's client at tick end.
fn sync_later(game: &mut Game<'_>) {
    if game.lan.is_none() {
        return;
    }
    let uuid = game.native_acting_id();
    let mut guard = shared_state();
    let state = guard.get_or_insert_with(State::default);
    if !state.sync.contains(&uuid) {
        state.sync.push(uuid);
    }
}

// ---------------------------------------------------------------------------
// The tick: reconcile the view slot, write the menu data
// ---------------------------------------------------------------------------

#[allow(private_interfaces)]
#[allow(private_interfaces)]
pub fn on_tick(game: &mut Game<'_>, pos: [i32; 3], entity: &mut ModBlockEntity) {
    if !is_drawer(&entity.id) {
        return;
    }
    if entity.items.is_empty() {
        entity.items.resize(1, None);
    }
    let (mut drawer, stored) = read_drawer(&entity.nbt);
    let max_stack = stored
        .as_ref()
        .map_or(64, |stack| max_stack_of(game, &stack.item));
    let signal_before = drawer.signal(max_stack);
    // 1. Reconcile what an outside actor did to the view slot (a hopper, the
    //    menu). 2. Apply our own queued left-click take. Order matters: the
    //    sync must not see our own take as an outside insert.
    let observed_key = entity.items[0].as_ref().map(stack_key);
    let observed = observed_key
        .as_deref()
        .zip(entity.items[0].as_ref().map(|stack| stack.count));
    let sync = drawer.sync_view(0, observed, max_stack);
    if sync.rejected.is_some()
        && let Some(stack) = entity.items[0].take()
    {
        eject(game, pos, stack);
    }
    let mut new_stored = match drawer.item(0) {
        None => None,
        Some(key) => match &stored {
            Some(stack) if stack_key(stack) == key => Some(stack.clone()),
            _ => entity.items[0].clone(),
        },
    };
    if drawer.item(0).is_none() {
        new_stored = None;
    }
    let item_changed = new_stored.as_ref().map(stack_key) != stored.as_ref().map(stack_key);
    write_drawer(&mut entity.nbt, &drawer, new_stored.as_ref(), max_stack);
    let view = view_stack(&drawer, new_stored.as_ref(), max_stack);
    if entity.items.first() != Some(&view) {
        entity.items[0] = view;
    }
    let data = vec![
        drawer.amount(0) as i32,
        drawer.limit(0, max_stack) as i32,
        if drawer.locked { 1 } else { 0 },
    ];
    if entity.data != data {
        entity.data = data;
    }
    if item_changed {
        game.native_block_entity_sync(pos);
    }
    if drawer.signal(max_stack) != signal_before {
        let dimension = game.native_dimension().to_owned();
        let mut guard = shared_state();
        let state = guard.get_or_insert_with(State::default);
        if !state
            .output
            .iter()
            .any(|(d, p)| *d == dimension && *p == pos)
        {
            state.output.push((dimension, pos));
        }
    }
}

// ---------------------------------------------------------------------------
// Client side: the front item and the screen
// ---------------------------------------------------------------------------

/// What clients get of a drawer (`native_block_entity_update_tag`, API 45):
/// the stored stack, so the front renderer can draw it.
#[allow(private_interfaces)]
pub fn on_update_tag(entity: &ModBlockEntity, tag: &mut Option<Compound>) {
    if !is_drawer(&entity.id) {
        return;
    }
    let mut out = Compound::new();
    if let Some(item) = entity.nbt.get(&JavaString::from(KEY_ITEM)) {
        out.insert(JavaString::from(KEY_ITEM), item.clone());
    }
    *tag = Some(out);
}

/// Draws the stored item at the drawer's front (`native_block_entity_submit`,
/// API 71). The pin has no in-world text helper, so the count is only in the
/// menu and the action bar.
#[allow(private_interfaces)]
pub fn on_submit(
    game: &Game<'_>,
    state: BlockStateId,
    entity: &ModBlockEntity,
    draw: &mut BlockEntityDraw,
) {
    if !is_drawer(&entity.id) {
        return;
    }
    let Some(Tag::Compound(item)) = entity.nbt.get(&JavaString::from(KEY_ITEM)) else {
        return;
    };
    let Some(stack) = item_from_nbt(&Tag::Compound(item.clone())) else {
        return;
    };
    let facing = game
        .native_block_state_property(state, "facing")
        .unwrap_or("north");
    let yaw = facing_yaw(facing);
    let offset = facing_offset(facing);
    draw.push();
    draw.translate([
        0.5 + offset[0] as f32 * 0.47,
        0.5,
        0.5 + offset[2] as f32 * 0.47,
    ]);
    draw.rotate_about([0.0, 1.0, 0.0], yaw, [0.0, 0.0, 0.0]);
    game.native_draw_item(draw, &stack, DisplayContext::Fixed, Some(yaw), 0);
    draw.pop();
}

/// The drawer's one-slot screen (`native_screen`, API 19).
#[allow(private_interfaces)]
pub fn on_screen(block: &str, screen: &mut Option<NativeScreen>) {
    if !is_drawer(block) {
        return;
    }
    *screen = Some(NativeScreen {
        width: 176.0,
        height: 166.0,
        slots: vec![Some([80.0, 20.0])],
        inventory_y: 84.0,
        inventory_x: 8.0,
        own_labels: false,
        // Horizon draws the plain panel and the slot frames (the drawer has no
        // GUI texture of its own).
        own_background: false,
    });
}

/// Draws the true count over the drawer screen (`native_screen_draw`, API 19):
/// the menu's data slots are `[amount, limit, locked]`.
#[allow(private_interfaces)]
pub fn on_screen_draw(
    game: &Game<'_>,
    block: &str,
    data: &[i32],
    batch: &mut HudBatch,
    origin: [f32; 2],
) {
    if !is_drawer(block) {
        return;
    }
    let amount = data.first().copied().unwrap_or(0);
    let limit = data.get(1).copied().unwrap_or(0);
    game.native_text(
        batch,
        &format!("{amount} / {limit}"),
        [origin[0] + 8.0, origin[1] + 40.0],
        0x404040,
        false,
    );
    if data.get(2).copied().unwrap_or(0) != 0 {
        game.native_text(
            batch,
            "Locked",
            [origin[0] + 8.0, origin[1] + 52.0],
            0x8b0000,
            false,
        );
    }
}

// ---------------------------------------------------------------------------
// Player interaction
// ---------------------------------------------------------------------------

/// Right click (`native_right_click_block`, API 21): the front face inserts the
/// held stack; a second click within 300 ms sweeps the whole inventory; sneak
/// with an empty hand toggles the lock; an empty hand opens the drawer menu.
#[allow(private_interfaces)]
pub fn on_right_click(
    game: &mut Game<'_>,
    pos: [i32; 3],
    face: [i32; 3],
    canceled: &mut bool,
) {
    let block = game.native_block(pos);
    if !is_drawer(&block) {
        return;
    }
    let Some(facing) = game.native_block_property(pos, "facing") else {
        return;
    };
    if facing_offset(&facing) != face {
        return;
    }
    let uuid = game.native_acting_id();
    let sneaking = game
        .player
        .as_ref()
        .is_some_and(|player| player.shift_key_down);
    let Some(held) = game.held().cloned() else {
        if sneaking
            && mutate(game, pos, |drawer, _| {
                drawer.toggle_locked();
                None
            })
        {
            *canceled = true;
            let locked = read_state(game, pos).is_some_and(|(drawer, _)| drawer.locked);
            game.native_action_bar(
                uuid,
                Text::literal(if locked {
                    "Drawer locked"
                } else {
                    "Drawer unlocked"
                }),
            );
        }
        return;
    };
    let key = stack_key(&held);
    let max_stack = max_stack_of(game, &held.item);
    let mut inserted = 0;
    let changed = mutate(game, pos, |drawer, stored| {
        inserted = drawer.insert(0, &key, held.count, max_stack);
        if inserted > 0 && stored.is_none() {
            let mut one = held.clone();
            one.count = 1;
            Some(one)
        } else {
            None
        }
    });
    if !changed {
        return;
    }
    if inserted == 0 {
        *canceled = true;
        game.native_action_bar(uuid, Text::literal("The drawer holds another item"));
        return;
    }
    let mut left = held;
    left.count -= inserted;
    *game.held_mut() = (left.count > 0).then_some(left);
    sync_later(game);
    *canceled = true;
    // Upstream's double-click window: a second click within 300 ms fills the
    // drawer from the whole inventory.
    let sweep = {
        let now = Instant::now();
        let mut guard = shared_state();
        let state = guard.get_or_insert_with(State::default);
        let sweep = state
            .last_click
            .get(&uuid)
            .is_some_and(|then| now.duration_since(*then) < Duration::from_millis(300));
        state.last_click.insert(uuid, now);
        sweep
    };
    if sweep {
        sweep_inventory(game, pos);
    }
    let (drawer, stored) = read_state(game, pos).unwrap_or_default();
    let amount = drawer.amount(0);
    let limit = stored.as_ref().map_or(capacity(64), |stack| {
        drawer.limit(0, max_stack_of(game, &stack.item))
    });
    game.native_action_bar(uuid, Text::literal(format!("{amount} / {limit}")));
}

/// Left click (`native_left_click_block`, API 40): take one (a stack while
/// sneaking) out of the front face.
#[allow(private_interfaces)]
pub fn on_left_click(
    game: &mut Game<'_>,
    pos: [i32; 3],
    face: [i32; 3],
    state: BlockStateId,
    canceled: &mut bool,
) {
    let block = game.res.facts().get(state).name.clone();
    if !is_drawer(&block) {
        return;
    }
    let Some(facing) = game.native_block_property(pos, "facing") else {
        return;
    };
    if facing_offset(&facing) != face {
        return;
    }
    // The pin keeps a block entity write only from `right_click_block` (and
    // the menu engine); every other hook tried here (this one, the block
    // entity tick, `player_tick`, `server_tick`, `level_tick_post`) reverts it,
    // so a left-click take would duplicate. Withdraw through the drawer's menu
    // (the one-slot menu is a result slot) instead.
    let uuid = game.native_acting_id();
    game.native_action_bar(uuid, Text::literal("Open the drawer to withdraw"));
    // The click is not cancelled: a held left button still mines the drawer.
    let _ = canceled;
}

/// Inserts every matching stack of the acting player's inventory (upstream's
/// 300 ms second click).
fn sweep_inventory(game: &mut Game<'_>, pos: [i32; 3]) {
    let Some((_, stored)) = read_state(game, pos) else {
        return;
    };
    let Some(stored) = stored else {
        return;
    };
    let key = stack_key(&stored);
    let max_stack = max_stack_of(game, &stored.item);
    let mut moved = false;
    for slot in 0..game.inventory.len() {
        let Some(stack) = game.inventory[slot].clone() else {
            continue;
        };
        if stack_key(&stack) != key {
            continue;
        }
        let mut take = 0;
        let _ = mutate(game, pos, |drawer, _| {
            take = drawer.insert(0, &key, stack.count, max_stack);
            None
        });
        if take == 0 {
            break;
        }
        let mut left = stack;
        left.count -= take;
        game.inventory[slot] = (left.count > 0).then_some(left);
        moved = true;
    }
    if let Some(offhand) = game.offhand.clone()
        && stack_key(&offhand) == key
    {
        let mut take = 0;
        let _ = mutate(game, pos, |drawer, _| {
            take = drawer.insert(0, &key, offhand.count, max_stack);
            None
        });
        if take > 0 {
            let mut left = offhand;
            left.count -= take;
            game.offhand = (left.count > 0).then_some(left);
            moved = true;
        }
    }
    if moved {
        sync_later(game);
    }
}

// ---------------------------------------------------------------------------
// Breaking and placing
// ---------------------------------------------------------------------------

/// A player breaks a drawer (`native_destroy_block`, API 14): keep its
/// contents for the loot drop, and empty the view slot so the container's
/// removal does not spill it. The break itself proceeds.
#[allow(private_interfaces)]
pub fn on_destroy_block(game: &mut Game<'_>, pos: [i32; 3], state: BlockStateId) {
    let block = game.res.facts().get(state).name.clone();
    if is_drawer(&block) {
        stash_bulk(game, pos);
    }
}

/// An explosion is about to break blocks (`native_explosion_detonate`, API 22):
/// keep the drawers' contents for their loot drops and empty their view slots.
#[allow(private_interfaces)]
pub fn on_explosion(game: &mut Game<'_>, blocks: &[[i32; 3]]) {
    for pos in blocks {
        if is_drawer(&game.native_block(*pos)) {
            stash_bulk(game, *pos);
        }
    }
}

fn stash_bulk(game: &mut Game<'_>, pos: [i32; 3]) {
    // The container slot spills its view on removal; the item component
    // carries what is left (the bulk), so the two add back to the true amount.
    // Clearing the view from this hook is reverted by the pin, so it is left
    // to the engine's spill.
    let data = game.native_with_mod_block_entity(pos, |entity| {
        if !is_drawer(&entity.id) || entity.nbt.get(&JavaString::from(KEY_ITEM)).is_none() {
            return None;
        }
        let view = entity
            .items
            .first()
            .and_then(Option::as_ref)
            .map_or(0, |stack| stack.count);
        let amount = match entity.nbt.get(&JavaString::from(KEY_AMOUNT)) {
            Some(Tag::Int(n)) if *n > 0 => *n as u32,
            _ => 0,
        };
        let bulk = amount.saturating_sub(view);
        if bulk == 0 {
            return None;
        }
        let mut data = drawer_data(&entity.nbt);
        data.insert(JavaString::from(KEY_AMOUNT), Tag::Int(bulk as i32));
        Some(data)
    });
    let Some(Some(data)) = data else {
        return;
    };
    let dimension = game.native_dimension().to_owned();
    let mut guard = shared_state();
    let state = guard.get_or_insert_with(State::default);
    state
        .stash
        .retain(|entry| !(entry.dimension == dimension && entry.pos == pos));
    state.stash.push(Stash {
        dimension,
        pos,
        data,
        at: Instant::now(),
    });
}

/// The saved contents of a drawer, as the item component carries them.
fn drawer_data(nbt: &Compound) -> Compound {
    let mut out = Compound::new();
    for key in [KEY_ITEM, KEY_AMOUNT, KEY_LOCKED, KEY_LIMIT] {
        if let Some(value) = nbt.get(&JavaString::from(key)) {
            out.insert(JavaString::from(key), value.clone());
        }
    }
    out
}

/// Attaches a broken drawer's contents to its item drop
/// (`native_loot_drops`, API 75).
#[allow(private_interfaces)]
pub fn on_loot_drops(
    game: &mut Game<'_>,
    context: &NativeLootDrops<'_>,
    drops: &mut Vec<ItemStack>,
) {
    let Some((pos, block_state)) = context.block else {
        return;
    };
    let name = game.res.facts().get(block_state).name.clone();
    if !is_drawer(&name) {
        return;
    }
    let Some(index) = drops.iter().position(|stack| stack.item.as_ref() == name.as_ref()) else {
        return;
    };
    let data = {
        let mut guard = shared_state();
        let state = guard.get_or_insert_with(State::default);
        let found = state
            .stash
            .iter()
            .position(|entry| entry.dimension == context.dimension && entry.pos == pos);
        found.map(|index| state.stash.remove(index).data)
    };
    let Some(data) = data else {
        return;
    };
    if data.is_empty() {
        return;
    }
    let _ = game.native_set_component(&mut drops[index], DRAWER_COMPONENT, Tag::Compound(data));
}

/// A drawer is placed from a stack (`native_place_block`, API 14): restore
/// the contents its component carries.
#[allow(private_interfaces)]
pub fn on_place_block(game: &mut Game<'_>, stack: &ItemStack, pos: [i32; 3]) {
    if !is_drawer(&stack.item) {
        return;
    }
    let data = match game.native_component(stack, DRAWER_COMPONENT) {
        Ok(Some(Tag::Compound(data))) => data.clone(),
        _ => return,
    };
    let _ = game.native_with_mod_block_entity(pos, |entity| {
        entity.nbt = data;
        if entity.items.is_empty() {
            entity.items.resize(1, None);
        }
    });
    game.native_block_entity_sync(pos);
}

// ---------------------------------------------------------------------------
// Comparator
// ---------------------------------------------------------------------------

#[allow(private_interfaces)]
pub fn on_has_analog_output(level: &GameLevel<'_>, state: BlockStateId, output: &mut bool) {
    if is_drawer(&level.facts.get(state).name) {
        *output = true;
    }
}

#[allow(private_interfaces)]
pub fn on_analog_output(
    level: &GameLevel<'_>,
    pos: horizon_core::position::BlockPos,
    output: &mut i32,
) {
    let key = (pos.x, pos.y, pos.z);
    let Some(BlockEntity::Mod(entity)) = level.world.block_entities.get(&key) else {
        return;
    };
    if !is_drawer(&entity.id) {
        return;
    }
    let (drawer, _) = read_drawer(&entity.nbt);
    let limit = match entity.nbt.get(&JavaString::from(KEY_LIMIT)) {
        Some(Tag::Int(n)) if *n > 0 => *n as u32,
        _ => capacity(64),
    };
    *output = drawer_signal(&drawer, limit);
}

// ---------------------------------------------------------------------------
// Server tick: deferred inventory syncs and comparator refreshes
// ---------------------------------------------------------------------------

#[allow(private_interfaces)]
pub fn on_server_tick(game: &mut Game<'_>, phase: ServerTickPhase) {
    if !matches!(phase, ServerTickPhase::ServerTickEnd { .. }) {
        return;
    }
    let sync = {
        let mut guard = shared_state();
        let state = guard.get_or_insert_with(State::default);
        // A break that never rolled its loot (a creative break) leaves a stale
        // stash; it is a tick old at most.
        let now = Instant::now();
        state
            .stash
            .retain(|entry| now.duration_since(entry.at) < Duration::from_secs(2));
        std::mem::take(&mut state.sync)
    };
    for uuid in sync {
        let Some(host) = game.lan.as_mut() else {
            break;
        };
        if let Some(index) = host
            .hub
            .guests
            .iter()
            .position(|guest| guest.placed && !guest.dead && guest.uuid == uuid)
        {
            host.hub.send_inventory(index);
        }
    }
}

/// After a level's tick pass (`native_level_tick_post`, API 4): a level is in
/// place, so the queued comparator refreshes and left-click takes can run.
#[allow(private_interfaces)]
pub fn on_level_tick_post(game: &mut Game<'_>, dimension: &str) {
    let positions = {
        let mut guard = shared_state();
        let state = guard.get_or_insert_with(State::default);
        let mut keep = Vec::new();
        let mut mine = Vec::new();
        for (d, pos) in std::mem::take(&mut state.output) {
            if d == dimension {
                mine.push(pos);
            } else {
                keep.push((d, pos));
            }
        }
        state.output = keep;
        mine
    };
    for pos in positions {
        game.native_update_output_neighbors(dimension, pos);
    }

}

// ---------------------------------------------------------------------------
// Commands and components
// ---------------------------------------------------------------------------

#[allow(private_interfaces)]
pub fn on_component_types(types: &mut Vec<String>) {
    types.push(DRAWER_COMPONENT.to_owned());
}

#[allow(private_interfaces)]
pub fn on_register_commands(commands: &mut Vec<NativeCommand>) {
    commands.push(NativeCommand::new("functionalstorage info"));
}

#[allow(private_interfaces)]
pub fn on_perform_command(
    game: &mut Game<'_>,
    _player: &NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    let words: Vec<&str> = command.split_whitespace().collect();
    if words.first() != Some(&"functionalstorage") {
        return;
    }
    *ran = true;
    let parsed = (2..5)
        .map(|index| words.get(index).and_then(|word| word.parse::<i32>().ok()))
        .collect::<Option<Vec<i32>>>();
    let Some(parsed) = parsed.filter(|_| words.get(1) == Some(&"info")) else {
        game.native_feedback(Text::literal(
            "usage: /functionalstorage info <x> <y> <z>",
        ));
        return;
    };
    let pos = [parsed[0], parsed[1], parsed[2]];
    let line = match read_state(game, pos) {
        Some((drawer, stored)) => {
            let max_stack = stored
                .as_ref()
                .map_or(64, |stack| max_stack_of(game, &stack.item));
            format!(
                "{}: {}/{} {}{}",
                game.native_block(pos),
                drawer.amount(0),
                drawer.limit(0, max_stack),
                stored
                    .as_ref()
                    .map_or("empty", |stack| stack.item.as_ref()),
                if drawer.locked { " locked" } else { "" }
            )
        }
        None => format!("{}: not a drawer", game.native_block(pos)),
    };
    game.native_feedback(Text::literal(line));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_facing_of_a_drawer_turns_with_the_placer() {
        assert_eq!(facing_for_yaw(0.0), "north");
        assert_eq!(facing_for_yaw(90.0), "east");
        assert_eq!(facing_for_yaw(180.0), "south");
        assert_eq!(facing_for_yaw(-90.0), "west");
    }

    #[test]
    fn the_front_face_is_the_facing_direction() {
        assert_eq!(facing_offset("north"), [0, 0, -1]);
        assert_eq!(facing_offset("east"), [1, 0, 0]);
        assert_eq!(facing_yaw("south"), 180.0);
    }

    #[test]
    fn a_stack_key_keeps_components_apart() {
        let a = ItemStack {
            item: "minecraft:stone".into(),
            count: 1,
            damage: 0,
            extra: None,
        };
        assert_eq!(stack_key(&a), stack_key(&a.clone()));
        let mut b = a.clone();
        b.damage = 1;
        assert_ne!(stack_key(&a), stack_key(&b));
    }
}
