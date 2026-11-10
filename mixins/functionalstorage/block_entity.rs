// Runs at the head of `Game::native_block_entity` (API 15): a drawer gets a
// block entity.
fn observe_block_entity(
    block: &str,
    entity: &mut Option<(String, horizon_world::nbt::Compound)>,
) {
    crate::game::functionalstorage_state::on_block_entity(block, entity);
}
