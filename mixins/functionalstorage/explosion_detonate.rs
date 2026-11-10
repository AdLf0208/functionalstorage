// Runs at the head of `Game::native_explosion_detonate` (API 22): keep the
// drawers' contents for their loot drops.
fn observe_explosion(
    game: &mut crate::game::Game<'_>,
    blocks: &mut Vec<[i32; 3]>,
) {
    crate::game::functionalstorage_state::on_explosion(game, blocks);
}
