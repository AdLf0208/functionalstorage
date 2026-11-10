// Runs at the head of `Game::native_state_for_placement` (API 76): a placed
// drawer faces its placer.
fn observe_state_for_placement(
    level: &crate::game::GameLevel<'_>,
    state: horizon_world::registry::BlockStateId,
    context: &PlacementContext,
    answer: &mut Option<Option<horizon_world::registry::BlockStateId>>,
) {
    crate::game::functionalstorage_state::on_state_for_placement(level, state, context, answer);
}
