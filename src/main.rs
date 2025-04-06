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
    mut q_spawned_fields: Query<(&Field, Entity)>,
) {
    if !available_hosts.is_changed() {
        return;
    }

    let old_hosts = q_spawned_fields
        .iter_mut()
        .map(|(field, entity)| (&field.host, entity))
        .collect::<Vec<_>>();

    // Remove fields of old hosts
    old_hosts
        .iter()
        .filter(|(old_host, _)| {
            available_hosts
                .0
                .iter()
                .all(|new_host| new_host != *old_host)
        })
        .for_each(|(_, removed_entity)| {
            commands.entity(*removed_entity).despawn_recursive();
        });

    // Spawn fields with update tasks for each new host
    // TODO: Spawn multiple hosts in a grid
    available_hosts
        .0
        .iter()
        .filter(|new_host| old_hosts.iter().all(|(old_host, _)| new_host != old_host))
        .for_each(|new_host| {
            commands.spawn(Field::bind(new_host.clone()));
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
