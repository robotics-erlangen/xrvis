use crate::hosts::{Host, HostConnection};
use crate::proto;
use crate::proto::vis_shape::Geom;
use bevy::prelude::*;
use derive_more::IntoIterator;

pub fn vis_plugin(app: &mut App) {
    app.add_systems(PostUpdate, send_vis_selection);
}

// ======== Visualization sources ========
/// ECS component for [`proto::VisSourceSkeleton`], managed by [`update_visualization_model`].
#[derive(Component, Clone, Debug, PartialEq)]
#[component(immutable)]
pub struct VisualizationSource {
    pub name: String,
}
impl VisualizationSource {
    fn new(name: String) -> Self {
        Self { name }
    }
}

/// ECS component for [`proto::VisSkeleton`], managed by [`update_visualization_model`].
/// This is just an identifier, the actual shape data is stored in [`ActiveVisualizationData`].
#[derive(Component, Clone, Debug, PartialEq)]
#[component(immutable)]
pub struct Visualization {
    pub id: u32,
    pub name: String,
}
impl Visualization {
    fn new(id: u32, name: String) -> Self {
        Self { id, name }
    }
}

/// The latest shape data for an active [`Visualization`], used to generate the mesh to display on fields.
///
/// A visualization is considered active if it contains at least one shape in this frame. But the
/// [`proto::VisualizationFilter`] makes this a bit more complicated, as it can cull shapes for
/// visualizations that are still active on the host.
///
/// So this component encodes three states:
/// - Present and non-empty: The visualization is active and unfiltered/used
/// - Present and empty: The visualization is active but filtered/unused
/// - Absent: The visualization is inactive
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ActiveVisualizationData(pub Vec<proto::VisShape>);

/// Vis-side counterpart to [`AllHostVisualizations`]. Direct link to the host of the visualization, skipping the source entity.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
#[relationship(relationship_target = AllHostVisualizations)]
pub struct VisualizationFromHost(pub Entity);
/// Host-side counterpart to [`VisualizationFromHost`]. Direct link to all visualizations of this host, skipping the source entities.
#[derive(Component, IntoIterator, Clone, Debug, PartialEq, Eq)]
#[relationship_target(relationship = VisualizationFromHost, linked_spawn)]
pub struct AllHostVisualizations(#[into_iterator(owned, ref, ref_mut)] Vec<Entity>);

/// Field-side counterpart to [`VisualizationUsages`]. Any entity with this component will automatically get its target's mesh.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
#[relationship(relationship_target = VisualizationUsages)]
#[require(Transform)]
pub struct VisualizationInstance(pub Entity);
/// Host-side counterpart to [`VisualizationInstance`]. Contains all entities that should be updated from this entity's [`ActiveVisualizationData`].
#[derive(Component, IntoIterator, Clone, Debug, PartialEq, Eq)]
#[relationship_target(relationship = VisualizationInstance, linked_spawn)]
pub struct VisualizationUsages(#[into_iterator(owned, ref, ref_mut)] Vec<Entity>);

/// Encodes a [`proto::VisModel`] for a single host into the ecs.
///
/// The generated structure looks like this:\
/// \- ([`Host`], [`HostConnection`], Option<[`AllHostVisualizations`]>)\
/// | \- [`VisualizationSource`]\
/// | | \- ([`Visualization`], Option<[`ActiveVisualizationData`]>, [`VisualizationFromHost`])
pub(crate) fn update_visualization_model(
    In((host_entity, vis_model)): In<(Entity, proto::VisModel)>,
    mut commands: Commands,
    mut q_hosts: Query<Option<&Children>, With<HostConnection>>,
    mut q_vis_sources: Query<(&VisualizationSource, Option<&Children>, Entity)>,
    mut q_visualizations: Query<(&Visualization, Entity)>,
) {
    let source_entities = match q_hosts.get_mut(host_entity) {
        Ok(c) => c,
        Err(e) => {
            error!(
                "Failed to fetch host entity {host_entity:?} for update_visualization_names: {e}"
            );
            return;
        }
    };

    let mut new_source_list = vis_model.groups;

    // Update/Remove existing sources
    let mut source_iter = q_vis_sources.iter_many_mut(source_entities.into_iter().flatten());
    while let Some(Ok((source, vis_entities, source_entity))) = source_iter.fetch_next() {
        // Get all new messages for this source. There should only be one, but that isn't actually
        // enforced in the protocol to make minimal host implementations easier.
        let mut source_skeletons = new_source_list.extract_if(.., |s| s.name == source.name);

        // Explicit handling for the first element to short-circuit without collecting into a vec
        let Some(first) = source_skeletons.next() else {
            commands.entity(source_entity).despawn();
            continue;
        };

        let mut vis_skeletons: Vec<_> = std::iter::once(first)
            .chain(source_skeletons)
            .flat_map(|a| a.visualizations)
            .collect();

        // Update/Remove existing visualizations
        let mut vis_iter = q_visualizations.iter_many_mut(vis_entities.into_iter().flatten());
        while let Some(Ok((vis_component, vis_entity))) = vis_iter.fetch_next() {
            if let Some(vis_skeleton) = vis_skeletons
                .extract_if(.., |v| v.id == vis_component.id)
                .next()
            {
                commands.entity(vis_entity).insert_if_neq(Visualization {
                    id: vis_skeleton.id,
                    name: vis_skeleton.name,
                });
            } else {
                commands.entity(vis_entity).despawn();
                continue;
            }
        }

        // Spawn new visualizations
        for vis_skeleton in vis_skeletons {
            commands.spawn((
                Visualization::new(vis_skeleton.id, vis_skeleton.name),
                VisualizationFromHost(host_entity),
                ChildOf(source_entity),
            ));
        }
    }

    // Spawn new sources
    for new_source in new_source_list {
        commands
            .spawn((
                VisualizationSource::new(new_source.name),
                ChildOf(host_entity),
            ))
            .with_children(|spawner| {
                for vis_skeleton in new_source.visualizations {
                    spawner.spawn((
                        Visualization::new(vis_skeleton.id, vis_skeleton.name),
                        VisualizationFromHost(host_entity),
                    ));
                }
            });
    }
}

/// Updates the visualization data for a single host. This system only manages the [`ActiveVisualizationData`]
/// for pre-existing entities, their creation is handled by [`update_visualization_model`].
#[allow(clippy::type_complexity)]
pub(crate) fn update_visualizations(
    In((host_entity, mut vis_update)): In<(Entity, proto::VisualizationUpdate)>,
    mut commands: Commands,
    q_hosts: Query<Option<&AllHostVisualizations>, With<HostConnection>>,
    mut q_visualizations: Query<(&Visualization, Option<&mut ActiveVisualizationData>, Entity)>,
) {
    let vis_entities = match q_hosts.get(host_entity) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to fetch host entity {host_entity:?} for update_visualizations: {e}");
            return;
        }
    };

    // Convert from the vision coordinate system (right-handed, z up, x towards blue goal, +x forward)
    // to bevy's coordinate system (right-handed, y up, x towards blue goal, -z forward) with y and z swapped
    for vis in vis_update.active_vis.iter_mut() {
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
    }

    let group_selector = vis_update
        .group_selector
        .unwrap_or(proto::VisGroupSelector {
            group: 0,
            group_count: 1,
        });

    let mut new_vis_list = vis_update.active_vis;

    let mut vis_iter = q_visualizations.iter_many_mut(vis_entities.into_iter().flatten());
    while let Some(Ok((vis, vis_data, vis_entity))) = vis_iter.fetch_next() {
        // Skip if the group doesn't match
        if vis.id % group_selector.group_count != group_selector.group {
            continue;
        }

        let shapes: Vec<_> = new_vis_list
            .extract_if(.., |v| v.id == vis.id)
            .flat_map(|v| v.shape)
            .collect();

        if shapes.is_empty() && !vis_update.hidden_active_vis.contains(&vis.id) {
            commands
                .entity(vis_entity)
                .remove::<ActiveVisualizationData>();
            continue;
        }

        match vis_data {
            Some(mut vis_data) => {
                vis_data.set_if_neq(ActiveVisualizationData(shapes));
            }
            None => {
                commands
                    .entity(vis_entity)
                    .insert(ActiveVisualizationData(shapes));
            }
        }
    }
}

/// Sends a [`proto::VisualizationFilter`](proto::VisualizationFilter) to the host, based on which visualization entities have [`VisualizationUsages`].
fn send_vis_selection(
    q_hosts: Query<(&Host, &HostConnection, &AllHostVisualizations)>,
    q_visualizations: Query<(&Visualization, Ref<VisualizationUsages>)>,
    mut removed_usages: RemovedComponents<VisualizationUsages>,
) {
    let last_usage_removed = removed_usages.read().collect::<Vec<_>>();

    for (host, host_connection, vis_entities) in q_hosts {
        let mut host_changed = false;

        let mut filter = proto::VisualizationFilter::default();

        if vis_entities.iter().any(|e| last_usage_removed.contains(&e)) {
            host_changed = true;
        }
        for (vis, vis_usages) in q_visualizations.iter_many(vis_entities).matched() {
            // Empty VisualizationUsages are automatically removed by the relationship hooks
            if vis_usages.is_changed() {
                host_changed = true;
            }

            filter.allowed_visualizations.push(vis.id);
        }

        if host_changed {
            debug!("Sending vis selection to host {}: {:?}", host, filter);
            _ = host_connection
                .sender
                .send_blocking(proto::ws_request::Content::SetVisFilter(filter));
        }
    }
}
