// Runs at the head of `Game::native_perform_command` (API 8): `/functionalstorage info`.
fn perform_command(
    game: &mut crate::game::Game<'_>,
    player: &NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    crate::game::functionalstorage_state::on_perform_command(game, player, command, ran);
}
