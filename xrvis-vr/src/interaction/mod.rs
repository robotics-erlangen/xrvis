use bevy::prelude::App;

pub mod input;
pub mod picking;

pub fn interaction_plugins(app: &mut App) {
    app.add_plugins(input::xr_input_plugin);
    app.add_plugins(picking::xr_picking_plugin);
}
