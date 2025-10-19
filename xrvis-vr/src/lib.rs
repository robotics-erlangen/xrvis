use bevy::render::view::NoIndirectDrawing;
use bevy::{prelude::*, render::pipelined_rendering::PipelinedRenderingPlugin};
use bevy_mod_openxr::add_xr_plugins;
use bevy_mod_openxr::exts::OxrExtensions;
use bevy_mod_openxr::features::fb_passthrough::OxrFbPassthroughPlugin;
use bevy_mod_openxr::init::OxrInitPlugin;
use bevy_mod_openxr::resources::OxrSessionConfig;
use bevy_mod_openxr::types::EnvironmentBlendMode;
use sslgame::{AvailableHosts, Field, VisSelection, ssl_game_plugin};
use std::collections::HashSet;

#[bevy_main]
pub fn main() -> AppExit {
    let mut app = App::new();

    // XR setup
    app.add_plugins(
        // Disabling pipelining can improve input latency at the cost of some performance
        add_xr_plugins(DefaultPlugins.build().disable::<PipelinedRenderingPlugin>()).set(
            OxrInitPlugin {
                exts: {
                    let mut exts = OxrExtensions::default();
                    exts.enable_fb_passthrough();
                    exts.enable_hand_tracking();
                    exts
                },
                ..default()
            },
        ),
    )
    .insert_resource(OxrSessionConfig {
        blend_mode_preference: vec![
            EnvironmentBlendMode::ALPHA_BLEND,
            EnvironmentBlendMode::OPAQUE,
        ],
        ..default()
    })
    .add_plugins(OxrFbPassthroughPlugin)
    .add_plugins(bevy_mod_xr::hand_debug_gizmos::HandGizmosPlugin)
    .insert_resource(ClearColor(Color::NONE));

    // App setup
    app.add_plugins(ssl_game_plugin)
        .add_systems(Update, |mut q_fields: Query<&mut VisSelection>| {
            for mut selection in q_fields.iter_mut() {
                selection.selected = selection.available.keys().copied().collect::<HashSet<_>>();
            }
        })
        .add_systems(Startup, setup)
        // Disable indirect drawing because its gpu culling breaks on quest and msaa because it is really inefficient on mobile
        .add_systems(Update, (disable_indirect, disable_msaa))
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

fn disable_indirect(
    mut commands: Commands,
    cameras: Query<Entity, (With<Camera>, Without<NoIndirectDrawing>)>,
) {
    for entity in cameras {
        commands.entity(entity).insert(NoIndirectDrawing);
    }
}

#[derive(Component)]
struct MsaaModified;

fn disable_msaa(
    mut commands: Commands,
    cameras: Query<Entity, (With<Camera>, Without<MsaaModified>)>,
) {
    for cam in cameras {
        commands.entity(cam).insert(Msaa::Off).insert(MsaaModified);
    }
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
