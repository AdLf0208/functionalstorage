// Runs at the head of `Game::native_server_tick` (API 4): resend inventories a
// drawer use changed and refresh comparator outputs.
fn observe_server_tick(game: &mut crate::game::Game<'_>, phase: ServerTickPhase) {
    crate::game::functionalstorage_state::on_server_tick(game, phase);
}
