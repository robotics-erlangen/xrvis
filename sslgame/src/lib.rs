pub mod proto {
    pub mod remote {
        include!(concat!(env!("OUT_DIR"), "/remote.rs"));
    }
}
mod depth_mask_material;
mod mesh_generators;
mod network_tasks;
mod visualization_tracker;
mod world_state_filter;

use crate::depth_mask_material::DepthMaskMaterial;
use crate::mesh_generators::*;
use crate::network_tasks::{UpdatePacket, host_discovery_task};
use crate::proto::remote::udp_stream_request::UdpStream;
use crate::proto::remote::vis_part::Geom;
use crate::proto::remote::ws_stream_request::WsStream;
use crate::proto::remote::{
    HostAdvertisement, UdpStreamRequest, VisualizationFilter, WsStreamRequest, ws_request,
};
use crate::visualization_tracker::VisualizationTracker;
use crate::world_state_filter::WorldStateFilter;
use async_channel::{Receiver, Sender};
use bevy::mesh::{CylinderAnchor, CylinderMeshBuilder, SphereKind, SphereMeshBuilder};
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use std::cmp::PartialEq;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;

pub fn ssl_game_plugin(app: &mut App) {
    // Resources
    app.insert_resource(RenderSettings {
        field: true,
        robots: RobotRenderSettings::Fallback,
        ball: true,
        visualizations: true,
    });

    app.add_plugins(MaterialPlugin::<DepthMaskMaterial>::default());

    let robot_mask_mesh = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(MeshBuilder::build(
            &CylinderMeshBuilder::new(0.09, 0.15, 32).anchor(CylinderAnchor::Bottom),
        ));
    let robot_mask_material = app
        .world_mut()
        .resource_mut::<Assets<DepthMaskMaterial>>()
        .add(DepthMaskMaterial {});
    app.insert_resource(RobotMaskMesh(robot_mask_mesh, robot_mask_material));

    // FIXME: Ball in the ground
    let ball_mesh = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(MeshBuilder::build(&SphereMeshBuilder::new(
            0.0215,
            SphereKind::Ico { subdivisions: 3 },
        )));
    let ball_material = app
        .world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial::from_color(Color::srgb_u8(255, 136, 0)));
    app.insert_resource(BallMesh(ball_mesh, ball_material));

    app.insert_resource(AvailableHosts::default());

    // Systems
    app.add_systems(
        Update,
        (
            (
                receive_host_advertisements,
                receive_field_updates,
                send_vis_selection,
                handle_render_settings_change.run_if(resource_changed::<RenderSettings>),
            ),
            (
                update_field_geometry,
                update_world_state,
                update_visualizations,
            ),
        )
            .chain(),
    );
}

// ======== Resources ========

#[derive(Resource, Debug, Default)]
pub struct AvailableHosts(pub HashSet<FieldHost>);

#[derive(Resource, Debug)]
struct HostDiscoveryTask {
    discovery_channel: Receiver<Vec<(SocketAddr, HostAdvertisement)>>,
    discovery_task: Task<()>,
}

#[derive(Clone, Debug, Default)]
pub enum RobotRenderSettings {
    #[default]
    Detailed,
    Fallback,
    Cutout,
    None,
}

#[derive(Resource, Clone, Debug)]
pub struct RenderSettings {
    pub field: bool,
    pub robots: RobotRenderSettings,
    pub ball: bool,
    pub visualizations: bool,
}

impl RenderSettings {
    pub fn full() -> Self {
        RenderSettings {
            field: true,
            robots: RobotRenderSettings::Detailed,
            ball: true,
            visualizations: true,
        }
    }
    pub fn ar() -> Self {
        RenderSettings {
            field: false,
            robots: RobotRenderSettings::Cutout,
            ball: false,
            visualizations: true,
        }
    }
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            field: true,
            robots: RobotRenderSettings::default(),
            ball: true,
            visualizations: true,
        }
    }
}

#[derive(Resource, Debug)]
struct RobotMaskMesh(Handle<Mesh>, Handle<DepthMaskMaterial>);

#[derive(Resource, Debug)]
struct BallMesh(Handle<Mesh>, Handle<StandardMaterial>);

// ======== Field connection components ========

#[derive(Component, Debug)]
#[require(
    Visibility,
    Transform,
    FieldGeometry,
    GameState,
    AvailableVisualizations,
    SelectedVisualizations,
    WorldStateFilter,
    VisualizationTracker
)]
pub struct Field {
    pub host: FieldHost,
    connection: FieldConnection,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldHost {
    pub websocket_addr: SocketAddr,
    pub hostname: Option<String>,
}

#[derive(Debug)]
struct FieldConnection {
    sender: Sender<ws_request::Content>,
    receiver: Receiver<UpdatePacket>,
    io_task: Task<()>,
}

impl Field {
    pub fn bind(host: FieldHost) -> Self {
        let (rx_sender, rx_receiver) = async_channel::bounded(100);
        let (tx_sender, tx_receiver) = async_channel::bounded(10);
        let state_rx_task = IoTaskPool::get().spawn(network_tasks::io_task(
            host.websocket_addr,
            rx_sender,
            tx_receiver,
        ));

        debug!(
            "Spawned new field for host {}{}",
            host.websocket_addr,
            host.hostname
                .as_ref()
                .map(|name| format!(" ({name})"))
                .unwrap_or_default()
        );

        tx_sender
            .send_blocking(ws_request::Content::WsStreamReq(WsStreamRequest {
                stream: vec![
                    WsStream::FieldGeometry as i32,
                    WsStream::GameState as i32,
                    WsStream::VisMappings as i32,
                ],
            }))
            .unwrap();
        tx_sender
            .send_blocking(ws_request::Content::UdpStreamReq(UdpStreamRequest {
                stream: vec![
                    UdpStream::WorldState as i32,
                    UdpStream::Visualizations as i32,
                ],
                port: 0,
            }))
            .unwrap();

        Field {
            host,
            connection: FieldConnection {
                sender: tx_sender,
                receiver: rx_receiver,
                io_task: state_rx_task,
            },
        }
    }
}

// ======== Field state components ========

#[derive(Component, Debug, Clone, PartialEq)]
pub struct FieldGeometry {
    pub play_area_size: Vec2,
    pub boundary_width: f32,
    pub defense_size: Vec2,
    pub goal_width: f32,
}

#[derive(Component, Debug, Default, Clone, PartialEq, Eq)]
pub struct GameState {
    pub yellow_team: String,
    pub blue_team: String,
}

#[derive(Component, Debug, Default)]
pub struct AvailableVisualizations {
    pub sources: HashMap<u32, String>,
    pub visualizations: HashMap<u32, String>,
}

#[derive(Component, Debug, Default, PartialEq)]
pub struct SelectedVisualizations(pub VisualizationFilter);

impl FieldGeometry {
    const DIV_A: Self = Self {
        play_area_size: Vec2::new(12.0, 9.0),
        boundary_width: 0.3,
        defense_size: Vec2::new(1.8, 3.6),
        goal_width: 1.8,
    };
    const DIV_B: Self = Self {
        play_area_size: Vec2::new(9.0, 6.0),
        boundary_width: 0.3,
        defense_size: Vec2::new(1.0, 2.0),
        goal_width: 1.0,
    };
}

impl Default for FieldGeometry {
    fn default() -> Self {
        Self::DIV_A
    }
}

// ======== Field content components =========

#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    #[default]
    Yellow,
    Blue,
}

#[derive(Component, Debug, Clone, Copy)]
#[require(Team, Transform)]
pub struct Robot(pub u8);

#[derive(Component, Debug, Clone, Copy)]
#[require(Transform)]
pub struct Ball;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(Transform)]
pub struct Visualization(pub u32);

// ======== Systems ========

/// Manages the HostDiscoveryTask and updates the AvailableHosts resource
fn receive_host_advertisements(
    mut commands: Commands,
    running_receiver: Option<Res<HostDiscoveryTask>>,
    mut available_hosts: ResMut<AvailableHosts>,
) {
    if let Some(discovery_task) = running_receiver {
        if discovery_task.discovery_task.is_finished() {
            commands.remove_resource::<HostDiscoveryTask>();
            error!("Host discovery task stopped");
            // A new task will be started next frame
        } else {
            // Handle the new host list if available. There should only ever be one at a time.
            if let Ok(new_hosts) = discovery_task.discovery_channel.try_recv() {
                let new_hosts = new_hosts
                    .into_iter()
                    .map(|(addr, adv)| {
                        let mut websocket_addr = addr;
                        websocket_addr.set_port(adv.websocket_port as u16);
                        FieldHost {
                            websocket_addr,
                            hostname: adv.hostname,
                        }
                    })
                    .collect::<HashSet<_>>();

                // Only update the resource (and trigger change detection) when the hosts have actually changed
                if new_hosts != available_hosts.0 {
                    available_hosts.0 = new_hosts;
                }
            }
        }
    } else {
        // Start a new discovery task
        let (tx, rx) = async_channel::bounded(5);
        let task = IoTaskPool::get().spawn(host_discovery_task(tx));
        commands.insert_resource(HostDiscoveryTask {
            discovery_channel: rx,
            discovery_task: task,
        });
        info!("Host discovery task started");
    }
}

fn receive_field_updates(
    mut commands: Commands,
    mut q_fields: Query<(
        &Field,
        &mut FieldGeometry,
        &mut GameState,
        &mut AvailableVisualizations,
        &mut WorldStateFilter,
        &mut VisualizationTracker,
        Entity,
    )>,
) {
    for (
        field,
        mut geom,
        mut game_state,
        mut vis_selection,
        mut world_state,
        mut vis_tracker,
        entity,
    ) in q_fields.iter_mut()
    {
        if field.connection.io_task.is_finished() {
            info!(
                "Connection to {} closed, despawning field entities",
                field.host.websocket_addr
            );
            commands.entity(entity).despawn();
            continue;
        }
        while let Ok(new_packet) = field.connection.receiver.try_recv() {
            // The host should only send geom and game state update when they actually changed, but its still safer to check ourselves
            match new_packet {
                UpdatePacket::FieldGeom(new_geom) => {
                    geom.set_if_neq(FieldGeometry {
                        play_area_size: Vec2::new(new_geom.field_size_x, new_geom.field_size_y),
                        boundary_width: new_geom.boundary_width.unwrap_or(0.0),
                        defense_size: Vec2::new(
                            new_geom
                                .defense_size_x
                                .unwrap_or(new_geom.field_size_x / 6.),
                            new_geom
                                .defense_size_y
                                .unwrap_or(new_geom.field_size_y / 3.),
                        ),
                        goal_width: new_geom.goal_width.unwrap_or(new_geom.field_size_y / 5.),
                    });
                }
                UpdatePacket::GameState(new_game_state) => {
                    game_state.set_if_neq(GameState {
                        yellow_team: new_game_state.yellow_team_name.unwrap_or_default(),
                        blue_team: new_game_state.blue_team_name.unwrap_or_default(),
                    });
                }
                UpdatePacket::VisMappings(new_vis_mappings) => {
                    vis_selection.sources = new_vis_mappings.source;
                    vis_selection.visualizations = new_vis_mappings.name;
                }
                UpdatePacket::WorldState(new_world_state) => {
                    world_state.push_packet(new_world_state);
                }
                UpdatePacket::VisualizationUpdate(vis_update) => {
                    vis_tracker.push_update(vis_update);
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

#[derive(Component)]
pub struct FieldModelInstance(bevy::scene::InstanceId);

#[allow(clippy::type_complexity)]
fn handle_render_settings_change(
    mut commands: Commands,
    mut scene_spawner: ResMut<SceneSpawner>,
    render_settings: Res<RenderSettings>,
    (q_fields, q_robots, _q_balls): (
        Query<(&FieldModelInstance, Entity)>,
        Query<Entity, With<Robot>>,
        Query<Entity, With<Ball>>,
    ),
) {
    // Remove all potentially outdated entities. They will be recreated automatically.
    // Does not affect visualizations and balls, as they get regenerated every frame anyways.
    if !render_settings.field {
        // The field entity is also used as a marker for data processing, so only the model is removed
        for (instance, field_entity) in q_fields {
            scene_spawner.despawn_instance(instance.0);
            _ = commands.entity(field_entity).remove::<FieldModelInstance>();
        }
    }
    q_robots.iter().for_each(|e| commands.entity(e).despawn());
}

// ======== Update the world from the state filter ========

#[allow(clippy::type_complexity)]
fn update_field_geometry(
    mut commands: Commands,
    mut scene_spawner: ResMut<SceneSpawner>,
    render_settings: Res<RenderSettings>,
    (mut mesh_assets, mut material_assets, mut scene_assets): (
        ResMut<Assets<Mesh>>,
        ResMut<Assets<StandardMaterial>>,
        ResMut<Assets<Scene>>,
    ),
    mut q_fields: Query<(Ref<FieldGeometry>, Option<&FieldModelInstance>, Entity)>,
) {
    for (field_geometry, model_instance, entity) in &mut q_fields {
        if render_settings.field && (field_geometry.is_changed() || model_instance.is_none()) {
            if let Some(instance) = model_instance {
                scene_spawner.despawn_instance(instance.0);
            }
            let field_model = field_mesh(&field_geometry, &mut mesh_assets, &mut material_assets);
            commands.entity(entity).insert(FieldModelInstance(
                scene_spawner.spawn_as_child(scene_assets.add(field_model), entity),
            ));
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_world_state(
    mut commands: Commands,
    render_settings: Res<RenderSettings>,
    asset_server: Res<AssetServer>,
    (ball_mesh, robot_mask_mesh): (Res<BallMesh>, Res<RobotMaskMesh>),
    (q_fields, mut q_robots, q_balls): (
        Query<(&WorldStateFilter, Entity)>,
        Query<(&Robot, &Team, &mut Transform, &ChildOf, Entity)>,
        Query<(&Transform, &ChildOf, Entity), (With<Ball>, Without<Robot>)>,
    ),
) {
    for (world_state_filter, field_entity) in &q_fields {
        let world_state = world_state_filter.current_world_state(false);

        // TODO: Correlate new to old balls and move them instead of recreating everything. Don't forget to update handle_render_settings_change
        // Despawn old balls
        q_balls
            .iter()
            .map(|(_, c, e)| (c.parent(), e))
            .filter(|(p, _)| *p == field_entity)
            .for_each(|(_, e)| {
                commands.entity(field_entity).remove_children(&[e]);
                commands.entity(e).despawn()
            });

        // Spawn new balls
        for new_ball in world_state.ball {
            let new_ball_pos = Vec3::new(new_ball.p_x, new_ball.p_z.unwrap_or(0.0), new_ball.p_y);

            let mut new_ball = commands.spawn((Ball, Transform::from_translation(new_ball_pos)));
            if render_settings.ball {
                new_ball.insert((
                    Mesh3d(ball_mesh.0.clone()),
                    MeshMaterial3d(ball_mesh.1.clone()),
                ));
            }
            let new_ball = new_ball.id();
            commands.entity(field_entity).add_child(new_ball);
        }

        // Update robots
        let mut leftover_robots = q_robots
            .iter_mut()
            .filter(|(_, _, _, c, _)| c.parent() == field_entity)
            .collect::<Vec<_>>();

        let mut update_robots = |team: Team, new_robots: Vec<proto::remote::Robot>| {
            for robot_update in new_robots {
                let leftover_index = leftover_robots
                    .iter()
                    .position(|(r, t, _, _, _)| **t == team && r.0 as u32 == robot_update.id);
                let new_robot_pos = Vec3::new(robot_update.p_x, 0.0, robot_update.p_y);

                if let Some(i) = leftover_index {
                    // Robot already exists -> update transform
                    let (_, _, mut t, _, _) = leftover_robots.remove(i);
                    t.translation = new_robot_pos;
                    t.rotation = Quat::from_rotation_y(robot_update.phi);
                } else {
                    // Add new robot
                    let mut new_robot = commands.spawn((
                        Robot(robot_update.id as u8),
                        team,
                        Transform {
                            translation: new_robot_pos,
                            rotation: Quat::from_rotation_y(robot_update.phi),
                            ..Transform::default()
                        },
                    ));
                    match render_settings.robots {
                        RobotRenderSettings::Detailed => todo!(),
                        RobotRenderSettings::Fallback => {
                            new_robot
                                .insert(SceneRoot(asset_server.load("robots/generic.glb#Scene0")));
                        }
                        RobotRenderSettings::Cutout => {
                            new_robot.insert((
                                Mesh3d(robot_mask_mesh.0.clone()),
                                MeshMaterial3d(robot_mask_mesh.1.clone()),
                            ));
                        }
                        RobotRenderSettings::None => {}
                    }
                    let new_robot_id = new_robot.id();
                    commands.entity(field_entity).add_child(new_robot_id);
                }
            }
        };

        update_robots(Team::Yellow, world_state.yellow_robot);
        update_robots(Team::Blue, world_state.blue_robot);

        // Despawn all remaining robots
        leftover_robots.into_iter().for_each(|(_, _, _, _, e)| {
            commands.entity(field_entity).remove_children(&[e]);
            commands.entity(e).despawn()
        });
    }
}

#[derive(Default)]
pub struct VisualizationModels {
    pub circle: HashMap<u32, AssetId<Mesh>>,
    pub polygon: HashMap<Vec<(i32, i32)>, AssetId<Mesh>>,
    pub path: HashMap<Vec<(i32, i32)>, AssetId<Mesh>>,
}

#[allow(clippy::type_complexity)]
fn update_visualizations(
    mut commands: Commands,
    mut vis_models: Local<VisualizationModels>,
    render_settings: Res<RenderSettings>,
    (mut mesh_assets, mut material_assets): (
        ResMut<Assets<Mesh>>,
        ResMut<Assets<StandardMaterial>>,
    ),
    (mut q_fields, q_visualizations): (
        Query<(&mut VisualizationTracker, &AvailableVisualizations, Entity)>,
        Query<(&Visualization, &ChildOf, Entity)>,
    ),
) {
    for (mut vis_tracker, vis_names, field_entity) in &mut q_fields {
        let (group_count, updated_groups, new_visualizations) = vis_tracker.visualization_updates();
        // No new visualizations -> skip field
        if new_visualizations.is_empty() {
            continue;
        }

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

        // Forget unused meshes that were unloaded by the bevy asset system
        // This does not affect the entities that were just despawned because that
        // command is deferred to the end of the system run.
        vis_models
            .circle
            .retain(|_, asset_id| mesh_assets.contains(*asset_id));
        vis_models
            .polygon
            .retain(|_, asset_id| mesh_assets.contains(*asset_id));
        vis_models
            .path
            .retain(|_, asset_id| mesh_assets.contains(*asset_id));

        if render_settings.visualizations {
            // Generate and Spawn new visualization meshes
            for visualization in new_visualizations {
                for part in &visualization.part {
                    let border_color = part.border_style.and_then(|s| s.color).map_or(
                        Color::srgba_u8(0, 0, 0, 255),
                        |c| {
                            Color::srgba_u8(c.red as u8, c.green as u8, c.blue as u8, c.alpha as u8)
                        },
                    );

                    let mat = StandardMaterial::from(border_color);

                    let (mesh, pos) = match part.geom.as_ref() {
                        Some(Geom::Circle(c)) => {
                            // Convert to integers to get more stable hashing
                            let mm_radius = (c.radius * 1000.0) as u32;
                            if let Some(handle) = vis_models.circle.get(&mm_radius) {
                                (
                                    mesh_assets.get_strong_handle(*handle).unwrap(),
                                    Vec2::new(c.p_x, c.p_y),
                                )
                            } else {
                                let new_handle =
                                    mesh_assets.add(circle_visualization_mesh(32, c.radius));
                                vis_models.circle.insert(mm_radius, new_handle.id());
                                (new_handle, Vec2::new(c.p_x, c.p_y))
                            }
                        }
                        Some(Geom::Polygon(poly)) if !poly.point.is_empty() => {
                            let s = Vec2::new(poly.point[0].x, poly.point[0].y);
                            let points = poly
                                .point
                                .iter()
                                .map(|p| Vec2::new(p.x - s.x, p.y - s.y))
                                .collect::<Vec<_>>();
                            // Convert to integers to get more stable hashing
                            let mm_points = poly
                                .point
                                .iter()
                                .map(|p| {
                                    (((p.x - s.x) * 1000.0) as i32, ((p.y - s.y) * 1000.0) as i32)
                                })
                                .collect::<Vec<_>>();
                            if let Some(handle) = vis_models.polygon.get(&mm_points) {
                                (mesh_assets.get_strong_handle(*handle).unwrap(), s)
                            } else {
                                let new_handle =
                                    mesh_assets.add(polygon_visualization_mesh(&points));
                                vis_models.polygon.insert(mm_points, new_handle.id());
                                (new_handle, s)
                            }
                        }
                        Some(Geom::Path(path)) if !path.point.is_empty() => {
                            let s = Vec2::new(path.point[0].x, path.point[0].y);
                            let points = path
                                .point
                                .iter()
                                .map(|p| Vec2::new(p.x - s.x, p.y - s.y))
                                .collect::<Vec<_>>();
                            // Convert to integers to get more stable hashing
                            let mm_points = points
                                .iter()
                                .map(|p| {
                                    (((p.x - s.x) * 1000.0) as i32, ((p.y - s.y) * 1000.0) as i32)
                                })
                                .collect::<Vec<_>>();
                            if let Some(asset_id) = vis_models.path.get(&mm_points) {
                                (mesh_assets.get_strong_handle(*asset_id).unwrap(), s)
                            } else {
                                let new_handle =
                                    mesh_assets.add(path_visualization_mesh(&points, 0.01));
                                vis_models.path.insert(mm_points, new_handle.id());
                                (new_handle, s)
                            }
                        }
                        None => {
                            warn!(
                                "Invalid visualization part in {}: No geometry",
                                vis_names
                                    .visualizations
                                    .get(&visualization.id)
                                    .unwrap_or(&visualization.id.to_string())
                            );
                            continue;
                        }
                        _ => {
                            warn!(
                                "Invalid visualization part in {}: Empty geometry",
                                vis_names
                                    .visualizations
                                    .get(&visualization.id)
                                    .unwrap_or(&visualization.id.to_string())
                            );
                            continue;
                        }
                    };

                    commands.entity(field_entity).with_child((
                        Visualization(visualization.id),
                        Transform::from_xyz(pos.x, 0.001, pos.y),
                        Mesh3d(mesh),
                        MeshMaterial3d(material_assets.add(mat)),
                    ));
                }
            }
        }
    }
}
