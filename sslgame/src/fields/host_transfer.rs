use crate::fields::Field;
use crate::hosts::data::{FieldGeometry, GameState};
use crate::hosts::visualizations::{VisualizationData, VisualizationInstance};
use crate::hosts::{GameStateHost, GeometryHost, HostConnection, UpdateHostDataSystemSet};
use crate::mesh_gen::field::field_mesh;
use crate::mesh_gen::vis::visualization_mesh;
use crate::{DefaultMaterial, RenderSettings};
use bevy::prelude::*;

/// Sets up the systems responsible for transferring host state to connected fields
pub fn host_field_transfer_plugin(app: &mut App) {
    app.add_systems(
        PreUpdate,
        transfer_game_state.after(UpdateHostDataSystemSet),
    );

    app.add_systems(
        PreUpdate,
        transfer_field_geometry.after(UpdateHostDataSystemSet),
    );

    app.add_systems(PreUpdate, update_visualization_instances);
}

/// Transfers the game state from hosts to all fields related via [GameStateHost]
fn transfer_game_state(
    mut commands: Commands,
    mut q_fields: Query<(&GameStateHost, Option<&mut GameState>, Entity), With<Field>>,
    q_hosts: Query<Ref<GameState>, (With<HostConnection>, Without<Field>)>,
) {
    for (GameStateHost(host_entity), field_game_state, field_entity) in q_fields.iter_mut() {
        let Ok(host_game_state) = q_hosts.get(*host_entity) else {
            // The HostConnection is probably missing, so it makes no sense to update the fields
            continue;
        };

        if host_game_state.is_changed()
            && let Some(mut field_game_state) = field_game_state
        {
            field_game_state.set_if_neq(host_game_state.as_ref().clone());
        } else if field_game_state.is_none() {
            commands
                .entity(field_entity)
                .insert(host_game_state.as_ref().clone());
        }
    }
}

/// Updates fields meshes based on the [FieldGeometry] of the fields's [GeometryHost]
#[allow(clippy::type_complexity)]
fn transfer_field_geometry(
    mut commands: Commands,
    render_settings: Res<RenderSettings>,
    white_material: Res<DefaultMaterial>,
    mut mesh_assets: ResMut<Assets<Mesh>>,
    mut q_fields: Query<(&GeometryHost, Has<Mesh3d>, Entity), With<Field>>,
    q_hosts: Query<Ref<FieldGeometry>, (With<HostConnection>, Without<Field>)>,
) {
    for (GeometryHost(host_entity), has_mesh, entity) in &mut q_fields {
        let Ok(host_geometry) = q_hosts.get(*host_entity) else {
            // The HostConnection is probably missing, so it makes no sense to update the fields
            continue;
        };

        if render_settings.field && (host_geometry.is_changed() || has_mesh) {
            commands.entity(entity).insert((
                Mesh3d(mesh_assets.add(field_mesh(&host_geometry))),
                MeshMaterial3d(white_material.opaque.clone()),
            ));
        }
    }
}

/// Updates the mesh of all [VisualizationInstance]s based on changes to their referenced [VisualizationData]
fn update_visualization_instances(
    mut commands: Commands,
    render_settings: Res<RenderSettings>,
    material: Res<DefaultMaterial>,
    mut mesh_assets: ResMut<Assets<Mesh>>,
    (q_vis_data, q_vis_instances): (
        Query<Ref<VisualizationData>>,
        Query<(&VisualizationInstance, Entity)>,
    ),
) {
    for (VisualizationInstance(vis_data_entity), vis_instance_entity) in q_vis_instances {
        let Ok(vis_data) = q_vis_data.get(*vis_data_entity) else {
            commands.entity(vis_instance_entity).remove::<Mesh3d>();
            continue;
        };

        if vis_data.is_changed() && render_settings.visualizations {
            let visualization = &vis_data.0;

            if !visualization.shape.is_empty() {
                let vis_mesh = mesh_assets.add(visualization_mesh(&[visualization]));

                commands.entity(vis_instance_entity).insert((
                    Mesh3d(vis_mesh),
                    MeshMaterial3d(material.translucent.clone()),
                ));
            }
        }
    }
}
