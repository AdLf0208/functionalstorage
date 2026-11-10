// Runs at the head of `Game::native_block_entity_update_tag` (API 45): clients
// get the stored stack so the front renderer can draw it.
fn observe_block_entity_update_tag(
    entity: &horizon_client::containers::ModBlockEntity,
    tag: &mut Option<horizon_world::nbt::Compound>,
) {
    crate::game::functionalstorage_state::on_update_tag(entity, tag);
}
