pub mod input;
pub mod panels;
pub mod picking;

pub fn interaction_plugins(app: &mut bevy::prelude::App) {
    app.add_plugins(input::xr_input_plugin);
    app.add_plugins(panels::xr_panel_plugin);
    app.add_plugins(picking::xr_picking_plugin);
}
