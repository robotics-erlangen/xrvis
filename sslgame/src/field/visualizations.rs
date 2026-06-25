use crate::field::Field;
use crate::mesh_gen::vis::visualization_mesh;
use crate::proto::remote::{VisualizationFilter, ws_request};
use crate::visualization_tracker::VisualizationTracker;
use crate::{DefaultMaterial, RenderSettings};
use bevy::asset::{AssetServer, Assets};
use bevy::math::{Quat, Vec3};
use bevy::mesh::{Mesh, Mesh3d};
use bevy::pbr::MeshMaterial3d;
use bevy::prelude::*;
use std::collections::HashMap;
use tracing::{debug, warn};

pub fn field_vis_plugin(app: &mut App) {
    app.add_systems(Update, update_visualizations);
    app.add_systems(PostUpdate, send_vis_selection);
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(Transform)]
pub struct Visualization(pub u32);

#[derive(Component, Debug, Default)]
pub struct AvailableVisualizations {
    pub sources: HashMap<u32, String>,
    pub visualizations: HashMap<u32, String>,
}

#[derive(Component, Debug, Default, PartialEq)]
pub struct SelectedVisualizations(pub VisualizationFilter);

#[allow(clippy::type_complexity)]
fn update_visualizations(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    render_settings: Res<RenderSettings>,
    material: Res<DefaultMaterial>,
    mut mesh_assets: ResMut<Assets<Mesh>>,
    (mut q_fields, q_visualizations): (
        Query<(&mut VisualizationTracker, &AvailableVisualizations, Entity)>,
        Query<(&Visualization, &ChildOf, Entity)>,
    ),
) {
    for (mut vis_tracker, vis_names, field_entity) in &mut q_fields {
        let Some((group_count, updated_groups, new_visualizations)) =
            vis_tracker.visualization_updates()
        else {
            // No new visualizations packets -> keep old visualizations
            continue;
        };

        // Despawn old visualization meshes
        q_visualizations
            .iter()
            .filter(|(_, c, _)| c.parent() == field_entity)
            .for_each(|(v, _, e)| {
                let group = v.0 % group_count;
                if updated_groups.contains(&group) {
                    commands.entity(e).despawn();
                }
            });

        if render_settings.visualizations {
            // Generate and Spawn new visualization meshes
            for visualization in new_visualizations {
                let vis_id = visualization.id;

                if !visualization.shape.is_empty() {
                    let vis_mesh =
                        mesh_assets.add(visualization_mesh(&visualization, Some(vis_names)));

                    commands.entity(field_entity).with_child((
                        Visualization(vis_id),
                        Transform::default(),
                        Mesh3d(vis_mesh),
                        MeshMaterial3d(material.opaque.clone()), // TODO: Switch back to translucent. Frustum culling for translucents with NoIndirectDrawing broke in bevy 0.19
                    ));
                }

                for asset_vis in visualization.asset {
                    if let Some(anim) = asset_vis.animation {
                        // TODO: Implement asset vis animations
                        warn!(
                            "Asset vis animations aren't implemented yet, tried playing {}:{anim}",
                            asset_vis.path
                        );
                    }
                    commands.entity(field_entity).with_child((
                        Visualization(vis_id),
                        Transform {
                            translation: asset_vis
                                .pos
                                .map(|p| Vec3::new(p.x, 0., p.y))
                                .unwrap_or(Vec3::ZERO),
                            rotation: Quat::from_rotation_y(asset_vis.angle.unwrap_or(0.0)),
                            scale: Vec3::ONE,
                        },
                        WorldAssetRoot(
                            // FIXME: Very easy path injection vulnerability (I guess there are already some others but this one seems especially obvious)
                            asset_server.load(format!("vis_assets/{}.glb#Scene0", asset_vis.path)),
                        ),
                    ));
                }
            }
        }
    }
}

fn send_vis_selection(
    q_fields: Query<(&Field, &SelectedVisualizations), Changed<SelectedVisualizations>>,
) {
    for (field, vis_selection) in q_fields {
        debug!("Sending vis selection: {:?}", vis_selection.0);
        _ = field
            .connection
            .sender
            .send_blocking(ws_request::Content::SetVisFilter(vis_selection.0.clone()));
    }
}
