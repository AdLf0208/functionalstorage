// Runs at the head of `Game::native_left_click_block` (API 40): take one item
// out of the front face.
fn observe_left_click(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    face: [i32; 3],
    state: horizon_world::registry::BlockStateId,
    canceled: &mut bool,
) {
    crate::game::functionalstorage_state::on_left_click(game, pos, face, state, canceled);
}
