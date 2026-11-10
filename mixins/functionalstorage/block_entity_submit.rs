// Runs at the head of `Game::native_block_entity_submit` (API 71): draw the
// stored item at the drawer's front.
fn observe_block_entity_submit(
    game: &crate::game::Game<'_>,
    block: &str,
    state: horizon_world::registry::BlockStateId,
    entity: &horizon_client::containers::ModBlockEntity,
    draw: &mut crate::game::native_block_entity_draw::BlockEntityDraw,
) {
    let _ = block;
    crate::game::functionalstorage_state::on_submit(game, state, entity, draw);
}
