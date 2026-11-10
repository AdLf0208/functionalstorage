// Runs at the head of `Game::native_has_analog_output` (API 64): a drawer
// answers a comparator.
fn observe_has_analog_output(
    level: &crate::game::GameLevel<'_>,
    state: horizon_world::registry::BlockStateId,
    output: &mut bool,
) {
    crate::game::functionalstorage_state::on_has_analog_output(level, state, output);
}
