// Runs at the head of `Game::native_register_commands` (API 8): the drawer
// readout used by players and the e2e.
fn register_commands(commands: &mut Vec<NativeCommand>) {
    crate::game::functionalstorage_state::on_register_commands(commands);
}
