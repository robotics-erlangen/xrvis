pub mod sslgame;

use crate::sslgame::{AvailableHosts, Field, VisSelection, ssl_game_plugin};
use bevy::prelude::*;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use std::collections::HashSet;

fn main() {
    let mut app = App::new();

    app.add_plugins(DefaultPlugins);
    app.add_plugins(ssl_game_plugin);

    // Dev plugins
    app.add_plugins(PanOrbitCameraPlugin);
    app.add_plugins(WorldInspectorPlugin::new());

    app.add_systems(Startup, test_init);
    app.add_systems(Update, |mut q_fields: Query<&mut VisSelection>| {
        for mut selection in q_fields.iter_mut() {
            selection.selected = selection.available.keys().copied().collect::<HashSet<_>>();
        }
    });
    app.add_systems(Update, spawn_new_hosts);

    app.run();
}

fn spawn_new_hosts(
    mut commands: Commands,
    available_hosts: Res<AvailableHosts>,
    mut q_spawned_fields: Query<Entity, With<Field>>,
) {
    if !available_hosts.is_changed() {
        return;
    }

    // Remove old fields
    q_spawned_fields
        .iter_mut()
        .for_each(|field_entity| commands.entity(field_entity).despawn_recursive());

    // Spawn fields for each new host in a grid
    available_hosts
        .0
        .iter()
        .enumerate()
        .for_each(|(i, new_host)| {
            let z_pos = (i * 10) as f32 - ((available_hosts.0.len() - 1) as f32 * 5.0);
            commands.spawn((
                Field::bind(new_host.clone()),
                Transform::from_xyz(0.0, 0.0, z_pos),
            ));
        });
}

fn test_init(mut commands: Commands) {
    commands.spawn((
        Transform::from_xyz(0.0, 8.0, 9.0),
        PanOrbitCamera::default(),
        bevy::core_pipeline::prepass::DepthPrepass,
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
