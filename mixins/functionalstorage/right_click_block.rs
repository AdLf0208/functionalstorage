// Runs at the head of `Game::native_right_click_block` (API 21): insert into
// the front face, toggle the lock, or let the menu open.
fn observe_right_click(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    face: [i32; 3],
    canceled: &mut bool,
) {
    crate::game::functionalstorage_state::on_right_click(game, pos, face, canceled);
}
