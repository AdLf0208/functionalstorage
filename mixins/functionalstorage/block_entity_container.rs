// Runs at the head of `Game::native_block_entity_container` (API 16/17): a
// drawer has one big slot behind a proportional view and a one-slot menu.
fn observe_container(
    block: &str,
    container: &mut Option<horizon_client::containers::ModContainer>,
) {
    crate::game::functionalstorage_state::on_container(block, container);
}
