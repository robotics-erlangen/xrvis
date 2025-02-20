pub mod sslgame;

use crate::sslgame::ssl_game_plugin;
use bevy::prelude::*;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};

fn main() {
    let mut app = App::new();

    app.add_plugins(DefaultPlugins);
    app.add_plugins(ssl_game_plugin);

    // Dev plugins
    app.add_plugins(PanOrbitCameraPlugin);
    app.add_plugins(WorldInspectorPlugin::new());

    app.add_systems(Startup, test_init);

    app.run();
}

fn test_init(mut commands: Commands) {
    commands.spawn((
        Transform::from_xyz(-5.0, 2.0, -5.0),
        PanOrbitCamera::default(),
    ));
    commands.spawn((
        Transform {
            translation: Vec3::new(0.0, 0.0, 5.0),  
            rotation: Quat::from_rotation_z(90.0_f32.to_radians()),
            ..Default::default()
        },
        DirectionalLight {
            color: Default::default(),
            illuminance: 1000.0,
            shadows_enabled: false,
            shadow_depth_bias: 0.0,
            shadow_normal_bias: 0.0,
        },
    ));
}
