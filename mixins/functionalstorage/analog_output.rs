// Runs at the head of `Game::native_analog_output` (API 64): the drawer's
// comparator signal is its fullness.
fn observe_analog_output(
    level: &crate::game::GameLevel<'_>,
    pos: horizon_core::position::BlockPos,
    output: &mut i32,
) {
    crate::game::functionalstorage_state::on_analog_output(level, pos, output);
}
