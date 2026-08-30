use crate::field::hosts::{Host, HostConnection};
use crate::mesh_gen::vis::visualization_mesh;
use crate::proto::remote::vis_shape::Geom;
use crate::proto::remote::{VisMappings, Visualization, VisualizationUpdate};
use crate::{DefaultMaterial, RenderSettings, proto};
use bevy::asset::{AssetServer, Assets};
use bevy::math::{Quat, Vec3};
use bevy::mesh::{Mesh, Mesh3d};
use bevy::pbr::MeshMaterial3d;
use bevy::prelude::*;
use derive_more::IntoIterator;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::f32::consts::PI;
use tracing::warn;

pub fn vis_plugin(app: &mut App) {
    app.add_systems(PreUpdate, update_visualization_instances);
    app.add_systems(PostUpdate, send_vis_selection);
}

/// Component that stores the latest visualization name mappings for a host, so that
/// new visualizations don't have to wait for a new packet to get their name.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct VisualizationNameMappings(pub VisMappings);

// ======== Visualization sources ========
/// The unique ID of this visualization source. Maps to a [VisualizationSourceName] through the [VisualizationNameMappings].
#[derive(Component, Clone, Debug, PartialEq)]
#[component(immutable)]
pub struct VisualizationSourceId(pub u32);
/// The human-readable name of a [VisualizationSourceId]. Created automatically from the host's [VisualizationNameMappings].
#[derive(Component, Clone, Debug, PartialEq)]
#[component(immutable)]
pub struct VisualizationSourceName(pub String);

// ======== Visualizations ========
/// The ID of this visualization, unique within its [VisualizationSourceId]. Maps to a [VisualizationName] through the [VisualizationNameMappings].
#[derive(Component, Clone, Debug, PartialEq)]
#[component(immutable)]
pub struct VisualizationId(pub u32);
/// The human-readable name of a [VisualizationId]. Created automatically from the host's [VisualizationNameMappings].
#[derive(Component, Clone, Debug, PartialEq)]
#[component(immutable)]
pub struct VisualizationName(pub String);
/// The latest data for a [VisualizationId]. Used to generate the mesh to display on fields.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct VisualizationData(Visualization);

/// Vis-side counterpart to [AllHostVisualizations]. Direct link to the host of the visualization, skipping the source entity.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
#[relationship(relationship_target = AllHostVisualizations)]
pub struct VisualizationFromHost(pub Entity);
/// Host-side counterpart to [VisualizationFromHost]. Direct link to all visualizations of this host, skipping the source entities.
#[derive(Component, IntoIterator, Clone, Debug, PartialEq, Eq)]
#[relationship_target(relationship = VisualizationFromHost, linked_spawn)]
pub struct AllHostVisualizations(#[into_iterator(owned, ref, ref_mut)] Vec<Entity>);

/// Field-side counterpart to [VisualizationUsages]. Any entity with this component will automatically get its target's asset children and mesh.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
#[relationship(relationship_target = VisualizationUsages)]
#[require(Transform)]
pub struct VisualizationInstance(pub Entity);
/// Host-side counterpart to [VisualizationInstance]. Contains all entities that should be updated from this entity's [VisualizationData].
#[derive(Component, IntoIterator, Clone, Debug, PartialEq, Eq)]
#[relationship_target(relationship = VisualizationInstance, linked_spawn)]
pub struct VisualizationUsages(#[into_iterator(owned, ref, ref_mut)] Vec<Entity>);

/// Marks that a visualization was not included in the last [VisualizationUpdate].
#[derive(Component)]
pub struct InactiveVisualization;

/// Adds a [VisualizationName]/[VisualizationSourceName] to each [VisualizationId]/[VisualizationSourceId]
/// that is present in the provided mappings, for a single host.
pub(crate) fn update_visualization_names(
    In((host_entity, vis_mappings)): In<(Entity, VisMappings)>,
    mut commands: Commands,
    mut q_hosts: Query<
        (Option<&mut VisualizationNameMappings>, Option<&Children>),
        With<HostConnection>,
    >,
    mut q_vis_sources: Query<(
        &VisualizationSourceId,
        Option<&VisualizationSourceName>,
        Option<&Children>,
        Entity,
    )>,
    mut q_visualizations: Query<(&VisualizationId, Option<&VisualizationName>, Entity)>,
) {
    let (cached_mappings, source_entities) = match q_hosts.get_mut(host_entity) {
        Ok(c) => c,
        Err(e) => {
            error!(
                "Failed to fetch host entity {host_entity:?} for update_visualization_names: {e}"
            );
            return;
        }
    };

    let mut source_iter = q_vis_sources.iter_many_mut(source_entities.into_iter().flatten());
    while let Some((source_id, source_name, vis_entities, source_entity)) = source_iter.fetch_next()
    {
        // Update source name
        if let Some(new_name) = vis_mappings.source.get(&source_id.0) {
            commands
                .entity(source_entity)
                .insert_if_neq(VisualizationSourceName(new_name.clone()));
        } else if source_name.is_some() {
            commands
                .entity(source_entity)
                .remove::<VisualizationSourceName>();
        }

        // Update visualization names
        let mut vis_iter = q_visualizations.iter_many_mut(vis_entities.into_iter().flatten());
        while let Some((vis_id, vis_name, vis_entity)) = vis_iter.fetch_next() {
            if let Some(new_name) = vis_mappings.name.get(&vis_id.0) {
                commands
                    .entity(vis_entity)
                    .insert_if_neq(VisualizationName(new_name.clone()));
            } else if vis_name.is_some() {
                commands.entity(vis_entity).remove::<VisualizationName>();
            }
        }
    }

    // Store the mappings so that new visualizations can immediately spawn with their name
    match cached_mappings {
        Some(mut m) => *m = VisualizationNameMappings(vis_mappings.clone()),
        None => {
            commands
                .entity(host_entity)
                .insert(VisualizationNameMappings(vis_mappings));
        }
    }
}

/// Updates the visualization entities for a single host. This system only provides the
/// [VisualizationId] and the [VisualizationData], the names are managed by [update_visualization_names].
/// Also note hosts with an active [proto::VisualizationFilter](crate::proto::remote::VisualizationFilter)
/// won't provide data for excluded entities, but they will still be present with an id.
/// The generated structure looks like this:\
/// \- ([Host], [HostConnection])\
/// | \- ([VisualizationSourceId], Option<[VisualizationSourceName]>)\
/// | | \- ([VisualizationId], Option<[VisualizationName]>, Option<[VisualizationData]>)
#[allow(clippy::type_complexity)]
pub(crate) fn update_visualizations(
    In((host_entity, mut vis_update)): In<(Entity, VisualizationUpdate)>,
    mut commands: Commands,
    q_hosts: Query<
        (&Host, Option<&VisualizationNameMappings>, Option<&Children>),
        With<HostConnection>,
    >,
    q_vis_sources: Query<(&VisualizationSourceId, Entity, Option<&Children>)>,
    mut q_visualizations: Query<(
        &VisualizationId,
        Option<&mut VisualizationData>,
        Option<&VisualizationUsages>,
        Entity,
    )>,
) {
    let (host, cached_names, source_entities) = match q_hosts.get(host_entity) {
        Ok((h, m, c)) => (h, m, c.into_iter().flatten()),
        Err(e) => {
            error!("Failed to fetch host entity {host_entity:?} for update_visualizations: {e}");
            return;
        }
    };

    // Convert from the vision coordinate system (right-handed, z up, x towards blue goal, +x forward)
    // to bevy's coordinate system (right-handed, y up, x towards blue goal, -z forward) with y and z swapped
    for vis in vis_update
        .visualization_set
        .iter_mut()
        .flat_map(|set| &mut set.visualization)
    {
        for part in &mut vis.shape {
            match &mut part.geom {
                Some(Geom::Circle(c)) => {
                    c.center.y = -c.center.y;
                }
                Some(Geom::Polygon(p)) => {
                    for point in &mut p.point {
                        point.y = -point.y;
                    }
                }
                Some(Geom::Path(p)) => {
                    for point in &mut p.point {
                        point.y = -point.y;
                    }
                }
                None => {}
            }
        }
        for asset in &mut vis.asset {
            if let Some(pos) = &mut asset.pos {
                pos.y = -pos.y;
            }
            if let Some(phi) = &mut asset.angle {
                *phi -= PI / 2.0;
            }
        }
    }

    let group_selector = vis_update
        .group_selector
        .unwrap_or(proto::remote::VisGroupSelector {
            group: 0,
            group_count: 1,
        });

    let mut new_source_list = vis_update.visualization_set;

    // Update the existing sources
    for (source_id, source_entity, vis_entities) in q_vis_sources.iter_many(source_entities) {
        // Get all new messages for this source. There should only be one, but that isn't actually
        // enforced in the protocol to make minimal host implementations easier.
        let new_source = new_source_list.extract_if(.., |vs| vs.source == source_id.0);
        // Sources without messages are kept, but all their inner visualizations will be marked as inactive

        // Merge the visualization messages from all the source messages
        let mut new_vis_map: HashMap<u32, Visualization> = HashMap::new();
        for vis in new_source.flat_map(|vs| vs.visualization) {
            match new_vis_map.entry(vis.id) {
                Entry::Occupied(mut entry) => {
                    let acc = entry.get_mut();
                    acc.shape.extend(vis.shape);
                    acc.asset.extend(vis.asset);
                    if acc.shape_theme != vis.shape_theme {
                        warn!(
                            "Got multiple different shape themes for vis {}, source {}, host {}. Using the first one:\n  first: {:?}\n  second: {:?}",
                            vis.id, source_id.0, host, acc.shape_theme, vis.shape_theme
                        );
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert(vis);
                }
            }
        }

        // Update the existing visualizations in this source
        let mut vis_iter = q_visualizations.iter_many_mut(vis_entities.into_iter().flatten());
        while let Some((vis_id, vis_data, vis_usages, vis_entity)) = vis_iter.fetch_next() {
            // Skip if the group doesn't match
            if vis_id.0 % group_selector.group_count != group_selector.group {
                continue;
            }

            let Some(new_vis) = new_vis_map.remove(&vis_id.0) else {
                // No messages for this vis -> mark inactive
                commands
                    .entity(vis_entity)
                    .insert(InactiveVisualization)
                    .remove::<VisualizationData>();
                continue;
            };
            commands
                .entity(vis_entity)
                .remove::<InactiveVisualization>();

            // Update vis data, but only if there is at least one usage
            let has_usage = vis_usages.is_some_and(|u| !u.0.is_empty());
            match (vis_data, has_usage) {
                (Some(mut vis_data), true) => {
                    // Update existing data
                    vis_data.set_if_neq(VisualizationData(new_vis));
                }
                (Some(_), false) => {
                    // Remove data because the visualization is currently unused
                    commands.entity(vis_entity).remove::<VisualizationData>();
                }
                (None, true) => {
                    // Insert new data
                    commands
                        .entity(vis_entity)
                        .insert(VisualizationData(new_vis));
                }
                (None, false) => {}
            }
        }

        // Spawn new entities for the remaining visualizations
        for new_vis in new_vis_map.into_values() {
            let name = cached_names.and_then(|n| n.0.name.get(&new_vis.id));
            // The name is spawned "atomically" with the id so that On<Added, VisId> observers see both
            let mut e = if let Some(name) = name {
                commands.spawn((
                    VisualizationId(new_vis.id),
                    VisualizationName(name.clone()),
                    VisualizationFromHost(host_entity),
                    ChildOf(source_entity),
                ))
            } else {
                commands.spawn((
                    VisualizationId(new_vis.id),
                    VisualizationFromHost(host_entity),
                    ChildOf(source_entity),
                ))
            };
            // Not having shapes or assets means that the visualization was probably excluded by a VisualizationFilter
            if !(new_vis.shape.is_empty() && new_vis.asset.is_empty()) {
                e.insert(VisualizationData(new_vis));
            }
        }
    }

    // Spawn the remaining sources
    while let Some(next_source) = new_source_list.first() {
        let next_source_id = next_source.source;
        new_source_list.retain(|vs| vs.source != next_source_id);

        let name = cached_names.and_then(|n| n.0.source.get(&next_source_id));
        // The name is spawned "atomically" with the id so that On<Added, SourceId> observers see both
        if let Some(name) = name {
            commands.spawn((
                VisualizationSourceId(next_source_id),
                VisualizationSourceName(name.clone()),
                ChildOf(host_entity),
            ));
        } else {
            commands.spawn((VisualizationSourceId(next_source_id), ChildOf(host_entity)));
        }

        // Visualizations for the new source will be filled in with the next update to reduce code duplication
    }
}

/// Updates the asset children and mesh of all [VisualizationInstance]s based on changes to their referenced [VisualizationData]
fn update_visualization_instances(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
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
                let vis_mesh = mesh_assets.add(visualization_mesh(visualization));

                commands.entity(vis_instance_entity).insert((
                    Mesh3d(vis_mesh),
                    MeshMaterial3d(material.opaque.clone()), // TODO: Switch back to translucent. Frustum culling for translucents with NoIndirectDrawing broke in bevy 0.19
                ));
            }

            for asset_vis in &visualization.asset {
                if let Some(anim) = &asset_vis.animation {
                    // TODO: Implement asset vis animations
                    warn!(
                        "Asset vis animations aren't implemented yet, tried playing {}:{anim}",
                        asset_vis.path
                    );
                }
                commands
                    .entity(vis_instance_entity)
                    .despawn_children()
                    .insert((
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

/// Sends a [proto::VisualizationFilter](crate::proto::remote::VisualizationFilter) to the host, based on which visualization entities have [VisualizationUsages].
fn send_vis_selection(
    q_hosts: Query<(&Host, &HostConnection, &Children)>,
    q_vis_sources: Query<(&VisualizationSourceId, &Children)>,
    q_visualizations: Query<(&VisualizationId, Ref<VisualizationUsages>)>,
    mut removed_usages: RemovedComponents<VisualizationUsages>,
) {
    let last_usage_removed = removed_usages.read().collect::<Vec<_>>();

    for (host, host_connection, source_entities) in q_hosts {
        let mut host_changed = false;

        let mut filter = proto::remote::VisualizationFilter::default();
        for (source_id, vis_entities) in q_vis_sources.iter_many(source_entities) {
            if vis_entities.iter().any(|e| last_usage_removed.contains(&e)) {
                host_changed = true;
            }

            for (vis_id, vis_usages) in q_visualizations.iter_many(vis_entities) {
                // Empty VisualizationUsages are automatically removed by the relationship hooks
                if vis_usages.is_changed() {
                    host_changed = true;
                }

                filter.allowed_visualizations.push(
                    proto::remote::visualization_filter::VisualizationFilterEntry {
                        source_id: Some(source_id.0),
                        vis_id: Some(vis_id.0),
                    },
                );
            }
        }

        if host_changed {
            debug!("Sending vis selection to host {}: {:?}", host, filter);
            _ = host_connection
                .sender
                .send_blocking(proto::remote::ws_request::Content::SetVisFilter(filter));
        }
    }
}
