// Runs at the head of `Game::native_block_entity_tick` (API 15): reconcile the
// container's view slot with the drawer's true amount.
fn tick(
    game: &mut crate::game::Game<'_>,
    pos: [i32; 3],
    entity: &mut horizon_client::containers::ModBlockEntity,
) {
    crate::game::functionalstorage_state::on_tick(game, pos, entity);
}
