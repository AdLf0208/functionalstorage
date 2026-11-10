// Runs at the head of `Game::native_destroy_block` (API 14): keep a drawer's
// contents for its loot drop and empty the view slot before the block goes.
fn observe_destroy_block(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    state: horizon_world::registry::BlockStateId,
) {
    crate::game::functionalstorage_state::on_destroy_block(game, pos, state);
}
