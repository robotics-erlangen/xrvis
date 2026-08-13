use bevy::prelude::*;
use std::collections::HashMap;
/*use bevy_nokhwa::BevyNokhwaPlugin;
use bevy_nokhwa::camera::BackgroundCamera;
use bevy_nokhwa::nokhwa::utils::{
    ApiBackend, CameraFormat, CameraIndex, FrameFormat, RequestedFormatType, Resolution,
};*/
use bevy_inspector_egui::bevy_egui;
use bevy_inspector_egui::bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass};
use bevy_inspector_egui::egui;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use sslgame::field::hosts::{
    BallHost, BlueRobotHost, GameStateHost, GeometryHost, Host, HostConnection, YellowRobotHost,
};
use sslgame::field::visualizations::{
    VisualizationId, VisualizationInstance, VisualizationName, VisualizationSourceId,
    VisualizationSourceName,
};
use sslgame::field::{Field, Team};
use sslgame::ssl_game_plugin;

fn main() {
    let mut app = App::new();

    app.add_plugins(DefaultPlugins);
    //app.add_plugins(BevyNokhwaPlugin);
    app.add_plugins(ssl_game_plugin);

    // Dev plugins
    app.add_plugins(PanOrbitCameraPlugin);
    app.insert_resource(EguiGlobalSettings {
        enable_absorb_bevy_input_system: true,
        ..Default::default()
    });
    app.add_plugins(EguiPlugin::default());
    app.add_plugins(WorldInspectorPlugin::new());
    app.add_systems(EguiPrimaryContextPass, vis_selection_ui);
    app.register_required_components::<Field, VisSelectionUiTab>();

    #[cfg(feature = "3d-panels")]
    {
        app.add_plugins(sslgame::panels::spatial_panel_plugin);
        app.add_plugins(sslgame::panels::game_state::game_state_panel_plugin);
    }

    app.add_systems(Startup, test_init);
    app.add_systems(Update, spawn_new_hosts);

    app.run();
}

fn spawn_new_hosts(
    mut commands: Commands,
    q_available_hosts: Query<(&Host, Option<&HostConnection>, Entity)>,
    mut q_spawned_fields: Query<Entity, With<Field>>,
) {
    if q_available_hosts.iter().all(|(_, conn, _)| conn.is_some()) {
        return;
    }

    // TODO: Don't respawn all fields because that also resets the selected visualizations
    // Remove old fields
    q_spawned_fields
        .iter_mut()
        .for_each(|field_entity| commands.entity(field_entity).despawn());

    // Spawn fields for each new host in a line. Sort by address to maintain a consistent order
    // of the remaining elements after one of them has been removed.
    let mut new_hosts = q_available_hosts.into_iter().collect::<Vec<_>>();
    new_hosts.sort_unstable_by_key(|(h, _, _)| h.websocket_addr);
    debug!("New Hosts: {:?}", new_hosts);
    new_hosts
        .iter()
        .enumerate()
        .for_each(|(i, (host, host_conn, host_entity))| {
            let z_pos = (i * 10) as f32 - ((new_hosts.len() - 1) as f32 * 5.0);
            if host_conn.is_none() {
                commands
                    .entity(*host_entity)
                    .insert(host.start_connection());
            }
            commands.spawn((
                Field,
                GeometryHost(*host_entity),
                GameStateHost(*host_entity),
                BallHost(*host_entity),
                YellowRobotHost(*host_entity),
                BlueRobotHost(*host_entity),
                Transform::from_xyz(0.0, 0.0, z_pos),
            ));
        });
}

#[derive(Component, Clone, Debug, Default)]
struct VisSelectionUiTab(Option<Team>);

#[allow(clippy::type_complexity)]
fn vis_selection_ui(
    mut commands: Commands,
    mut contexts: bevy_egui::EguiContexts,
    mut q_fields: Query<
        (
            Option<&YellowRobotHost>,
            Option<&BlueRobotHost>,
            &mut VisSelectionUiTab,
            Option<&Children>,
            Entity,
        ),
        With<Field>,
    >,
    q_hosts: Query<Option<&Children>, With<HostConnection>>,
    q_sources: Query<(
        &VisualizationSourceId,
        Option<&VisualizationSourceName>,
        &Children,
    )>,
    q_visualizations: Query<(&VisualizationId, Option<&VisualizationName>, Entity)>,
    q_visualization_instances: Query<(&VisualizationInstance, Entity)>,
) -> Result {
    egui::Window::new("Visualizations")
        .scroll([false, true])
        .collapsible(true)
        .resizable(true)
        .show(contexts.ctx_mut()?, |ui| {
            for (yellow_host_ref, blue_host_ref, mut ui_tab, vis_instances, field_entity) in
                q_fields.iter_mut()
            {
                // Map from each (host-side) visualization entity to its instance on this field
                let vis_to_instance = vis_instances
                    .into_iter()
                    .flatten()
                    .filter_map(|instance_entity| {
                        if let Ok((instance, _)) = q_visualization_instances.get(*instance_entity) {
                            Some((instance.0, instance_entity))
                        } else {
                            None
                        }
                    })
                    .collect::<HashMap<_, _>>();

                let mut draw_vis_toggles = |host_entity: Entity, ui: &mut egui::Ui| {
                    let source_entities = match q_hosts.get(host_entity) {
                        Ok(c) => c.into_iter().flatten(),
                        Err(e) => {
                            error!("Failed to fetch host entity {host_entity:?}: {e}");
                            return;
                        }
                    };
                    for (source_id, source_name, vis_entities) in
                        q_sources.iter_many(source_entities)
                    {
                        // Source name label
                        if let Some(source_name) = source_name {
                            ui.label(&source_name.0);
                        } else {
                            ui.label(format!("Source {}", source_id.0));
                        }

                        let mut flags: Vec<(String, Entity, bool)> = q_visualizations
                            .iter_many(vis_entities)
                            .map(|(vis_id, vis_name, vis_entity)| {
                                (
                                    if let Some(vis_name) = vis_name {
                                        vis_name.0.clone()
                                    } else {
                                        format!("Visualization {}", vis_id.0)
                                    },
                                    vis_entity,
                                    vis_to_instance.contains_key(&vis_entity),
                                )
                            })
                            .collect();
                        flags.sort_by(|(name_a, ..), (name_b, ..)| name_a.cmp(name_b));

                        for (name, vis_entity, checked) in flags.iter_mut() {
                            let was_checked = *checked;
                            ui.checkbox(checked, name.as_str());
                            if was_checked && !*checked {
                                if let Some(instance_entity) = vis_to_instance.get(vis_entity) {
                                    commands.entity(**instance_entity).despawn();
                                }
                            } else if !was_checked && *checked {
                                commands.spawn((
                                    VisualizationInstance(*vis_entity),
                                    ChildOf(field_entity),
                                ));
                            }
                        }

                        ui.separator();
                    }
                };

                // Draw "tabs" for each host
                // TODO: Add optional team hint to the vis sources proto and filter them here based on that
                match (yellow_host_ref.map(|h| h.0), blue_host_ref.map(|h| h.0)) {
                    (None, None) => {
                        ui.label("Field with no robot hosts!");
                        continue;
                    }
                    (Some(yellow_host_entity), None) => {
                        ui_tab.0 = Some(Team::Yellow);
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut ui_tab.0, Some(Team::Yellow), "Yellow");
                        });
                        ui.separator();
                        draw_vis_toggles(yellow_host_entity, ui);
                    }
                    (None, Some(blue_host_entity)) => {
                        ui_tab.0 = Some(Team::Blue);
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut ui_tab.0, Some(Team::Blue), "Blue");
                        });
                        ui.separator();
                        draw_vis_toggles(blue_host_entity, ui);
                    }
                    (Some(yellow_host_entity), Some(blue_host_entity))
                        if yellow_host_entity == blue_host_entity =>
                    {
                        ui_tab.0 = None;
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut ui_tab.0, None, "Yellow + Blue");
                        });
                        ui.separator();
                        draw_vis_toggles(yellow_host_entity, ui);
                    }
                    (Some(yellow_host_entity), Some(blue_host_entity)) => {
                        if ui_tab.0.is_none() {
                            ui_tab.0 = Some(Team::Yellow);
                        }
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut ui_tab.0, Some(Team::Yellow), "Yellow");
                            ui.selectable_value(&mut ui_tab.0, Some(Team::Blue), "Blue");
                        });
                        ui.separator();

                        match ui_tab.0 {
                            Some(Team::Yellow) => draw_vis_toggles(yellow_host_entity, ui),
                            Some(Team::Blue) => draw_vis_toggles(blue_host_entity, ui),
                            None => unreachable!(),
                        }
                    }
                }
            }
        });
    Ok(())
}

fn test_init(mut commands: Commands) {
    commands.spawn((
        Transform::from_xyz(0.0, 8.0, 9.0),
        PanOrbitCamera::default(),
        bevy::core_pipeline::prepass::DepthPrepass,
        /*BackgroundCamera::new(
            ApiBackend::Auto,
            Some(CameraIndex::Index(0)),
            Some(RequestedFormatType::Closest(CameraFormat::new(
                Resolution::new(1920, 1080),
                FrameFormat::MJPEG,
                60,
            ))),
        )
        .unwrap(),*/
    ));
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
}
