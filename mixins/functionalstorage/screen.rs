// Runs at the head of `Game::native_screen` (API 19): the drawer's one-slot
// screen layout.
fn observe_screen(block: &str, screen: &mut Option<NativeScreen>) {
    crate::game::functionalstorage_state::on_screen(block, screen);
}
