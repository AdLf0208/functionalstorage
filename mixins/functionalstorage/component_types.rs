// Runs before content loads: registers the drawer's contents component.
fn register_component_types(types: &mut Vec<String>) {
    crate::game::functionalstorage_state::on_component_types(types);
}
