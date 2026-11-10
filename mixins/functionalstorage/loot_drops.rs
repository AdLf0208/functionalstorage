// Runs at the head of `Game::native_loot_drops` (API 75): attach a broken
// drawer's contents to its item drop.
fn observe_loot_drops(
    game: &mut crate::game::Game<'_>,
    context: &NativeLootDrops<'_>,
    drops: &mut Vec<ItemStack>,
) {
    crate::game::functionalstorage_state::on_loot_drops(game, context, drops);
}
