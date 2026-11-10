// Runs at the head of `Game::native_place_block` (API 14): restore a drawer's
// contents from the item's component.
fn observe_place_block(
    game: &mut crate::game::Game<'_>,
    stack: &horizon_client::items::ItemStack,
    pos: [i32; 3],
) {
    crate::game::functionalstorage_state::on_place_block(game, stack, pos);
}
