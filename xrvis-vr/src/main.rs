use bevy::{prelude::*, render::pipelined_rendering::PipelinedRenderingPlugin};
use bevy_mod_openxr::add_xr_plugins;
use sslgame::{AvailableHosts, Field, VisSelection, ssl_game_plugin};
use std::collections::HashSet;

fn main() -> AppExit {
    let mut app = App::new();

    // XR setup
    app.add_plugins(
        // Disabling pipelining can improve input latency at the cost of some performance
        add_xr_plugins(DefaultPlugins.build().disable::<PipelinedRenderingPlugin>()),
    )
    .add_plugins(bevy_mod_xr::hand_debug_gizmos::HandGizmosPlugin)
    .insert_resource(ClearColor(Color::NONE));

    // App setup
    app.add_plugins(ssl_game_plugin)
        .add_systems(Startup, setup)
        .add_systems(Update, |mut q_fields: Query<&mut VisSelection>| {
            for mut selection in q_fields.iter_mut() {
                selection.selected = selection.available.keys().copied().collect::<HashSet<_>>();
            }
        })
        .add_systems(
            Update,
            spawn_new_hosts.run_if(resource_changed::<AvailableHosts>),
        )
        .insert_resource(AmbientLight {
            color: Default::default(),
            brightness: 500.0,
            affects_lightmapped_meshes: false,
        });

    app.run()
}

// Temporarily use the basic field distribution from the desktop verison
fn spawn_new_hosts(
    mut commands: Commands,
    available_hosts: Res<AvailableHosts>,
    mut q_spawned_fields: Query<Entity, With<Field>>,
) {
    // Remove old fields
    q_spawned_fields
        .iter_mut()
        .for_each(|field_entity| commands.entity(field_entity).despawn());

    // Spawn fields for each new host in a line. Sort by address to maintain a consistent order
    // of the remaining elements after one of them has been removed.
    let mut new_hosts = available_hosts.0.iter().collect::<Vec<_>>();
    new_hosts.sort_unstable_by_key(|h| h.addr);
    debug!("New Hosts: {:?}", new_hosts);
    new_hosts.into_iter().enumerate().for_each(|(i, new_host)| {
        let z_pos = (i * 10) as f32 - ((available_hosts.0.len() - 1) as f32 * 5.0);
        commands.spawn((
            Field::bind(new_host.clone()),
            Transform::from_xyz(0.0, 0.0, z_pos),
        ));
    });
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Floor
    let mut white: StandardMaterial = Color::WHITE.into();
    white.unlit = true;
    commands.spawn((
        Mesh3d(meshes.add(Circle::new(1.5))),
        MeshMaterial3d(materials.add(white)),
        Transform {
            translation: Vec3::new(0., -0.001, 0.),
            rotation: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            ..Transform::IDENTITY
        },
    ));
}
