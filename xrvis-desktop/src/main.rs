mod icons;
mod sidebar;
mod viewport;

use bevy::feathers::FeathersPlugins;
use bevy::feathers::dark_theme::create_dark_theme;
use bevy::feathers::theme::UiTheme;
use bevy::prelude::*;
use sslgame::ssl_game_plugin;

fn main() {
    let mut app = App::new();

    app.add_plugins((DefaultPlugins, FeathersPlugins));
    app.add_plugins(ssl_game_plugin);

    app.insert_resource(UiTheme(create_dark_theme()));
    app.add_plugins(icons::icons_plugin);
    app.add_plugins(sidebar::sidebar_plugin);
    app.add_plugins(viewport::viewport_plugin);

    // Dev plugins
    /*app.insert_resource(bevy_inspector_egui::bevy_egui::EguiGlobalSettings {
        enable_absorb_bevy_input_system: true,
        ..Default::default()
    });
    app.add_plugins(bevy_inspector_egui::bevy_egui::EguiPlugin::default());
    app.add_plugins(bevy_inspector_egui::quick::WorldInspectorPlugin::new());*/

    #[cfg(feature = "3d-panels")]
    {
        app.add_plugins(sslgame::panels::spatial_panel_plugin);
        app.add_plugins(sslgame::panels::game_state::game_state_panel_plugin);
    }

    app.add_systems(Startup, startup);

    app.run();
}

fn startup(mut commands: Commands) {
    commands.spawn((
        Transform {
            translation: Vec3::new(0.0, 5.0, 5.0),
            rotation: Quat::from_rotation_z(90.0_f32.to_radians()),
            ..Default::default()
        },
        DirectionalLight {
            illuminance: 1000.0,
            ..DirectionalLight::default()
        },
    ));

    // Spawn UI
    commands.spawn(Camera2d);
    commands.spawn_scene(bsn! {
        Node {
            width: percent(100),
            height: percent(100),
        }
        Children [
            sidebar::scene(),
            viewport::scene(),
        ]
    });
}
