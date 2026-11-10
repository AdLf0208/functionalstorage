// Runs at the head of `Game::native_level_tick_post` (API 4): the level is in
// place, so the queued comparator refreshes reach their neighbours.
fn observe_level_tick_post(game: &mut crate::game::Game<'_>, dimension: &str) {
    crate::game::functionalstorage_state::on_level_tick_post(game, dimension);
}
