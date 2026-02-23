use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::prelude::*;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::render::view::NoIndirectDrawing;
use bevy_mod_openxr::add_xr_plugins;
use bevy_mod_openxr::exts::OxrExtensions;
use bevy_mod_openxr::features::fb_passthrough::OxrFbPassthroughPlugin;
use bevy_mod_openxr::init::OxrInitPlugin;
use bevy_mod_openxr::resources::OxrSessionConfig;
use bevy_mod_openxr::types::EnvironmentBlendMode;
use sslgame::proto::remote::VisualizationFilter;
use sslgame::{
    AvailableHosts, AvailableVisualizations, Field, SelectedVisualizations, ssl_game_plugin,
};

mod interaction;
mod interaction_old;

#[bevy_main]
pub fn main() -> AppExit {
    let mut app = App::new();

    // XR setup
    app.add_plugins(
        // Disabling pipelining improves input latency at the cost of some performance
        add_xr_plugins(DefaultPlugins.build().disable::<PipelinedRenderingPlugin>()).set(
            OxrInitPlugin {
                exts: {
                    let mut exts = OxrExtensions::default();
                    exts.ext_hand_interaction = true;
                    exts.ext_hand_tracking = true;
                    exts.fb_passthrough = true;
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
        .add_systems(
            Update,
            |mut q_fields: Query<
                (&AvailableVisualizations, &mut SelectedVisualizations),
                Changed<AvailableVisualizations>,
            >| {
                for (available, mut selected) in q_fields.iter_mut() {
                    let new_filter = VisualizationFilter {
                        allowed_vis_source: available.sources.keys().copied().collect(),
                        allowed_vis_id: available
                            .visualizations
                            .iter()
                            .filter(|(id, name)| {
                                let name_lower = name.to_ascii_lowercase();
                                !name_lower.contains("zone") && !name_lower.contains("obstacle")
                            })
                            .map(|(id, _)| *id)
                            .collect(),
                    };
                    selected.set_if_neq(SelectedVisualizations(new_filter));
                }
            },
        )
        .add_plugins(interaction::interaction_plugins)
        .add_plugins(interaction_old::old_interaction_plugin)
        .add_systems(Startup, setup)
        .add_systems(Update, modify_cameras)
        .add_systems(
            Update,
            spawn_new_hosts.run_if(resource_changed::<AvailableHosts>),
        )
        .insert_resource(GlobalAmbientLight {
            color: Default::default(),
            brightness: 500.0,
            affects_lightmapped_meshes: false,
        });

    app.run()
}

#[derive(Component)]
struct CameraModified;

fn modify_cameras(
    mut commands: Commands,
    cameras: Query<Entity, (With<Camera>, Without<CameraModified>)>,
) {
    for cam in cameras {
        commands
            .entity(cam)
            // The depth prepass is required for robot cutouts
            .insert(DepthPrepass)
            // Gpu culling breaks on quest
            .insert(NoIndirectDrawing)
            // Bevy's MSAA is really inefficient on mobile
            .insert(Msaa::Off)
            .insert(CameraModified);
    }
}

fn spawn_new_hosts(
    mut commands: Commands,
    available_hosts: Res<AvailableHosts>,
    q_spawned_field: Option<Single<(&Field, Entity)>>,
) {
    let new_hosts = &available_hosts.0;

    if let Some(new_host) = new_hosts.iter().next() {
        match q_spawned_field.as_deref() {
            // Replace the field if it is not one of the new hosts, but a different one is there to replace it
            Some((field, entity))
                if !new_hosts
                    .iter()
                    .any(|h| field.host.websocket_addr == h.websocket_addr) =>
            {
                commands.entity(*entity).despawn();
                commands.spawn((Field::bind((*new_host).clone()), Transform::IDENTITY));
            }
            // Spawn a new field if there isn't one currently spawned
            None => {
                commands.spawn((Field::bind((*new_host).clone()), Transform::IDENTITY));
            }
            _ => {}
        }
    }
}

fn setup(mut commands: Commands, mut gizmo_assets: ResMut<Assets<GizmoAsset>>) {
    // Origin marker
    let mut asset = GizmoAsset::new();
    asset.circle(
        Isometry3d::IDENTITY,
        0.1,
        bevy::color::palettes::basic::WHITE,
    );
    commands.spawn((
        Gizmo {
            handle: gizmo_assets.add(asset),
            ..default()
        },
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));
}
