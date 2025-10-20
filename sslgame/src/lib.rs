pub mod proto {
    pub mod status_streaming {
        include!(concat!(env!("OUT_DIR"), "/status_streaming.rs"));
    }
}
mod depth_mask_material;
mod mesh_generators;
mod network_tasks;
mod state_filter;

use crate::depth_mask_material::DepthMaskMaterial;
use crate::mesh_generators::*;
use crate::network_tasks::host_discovery_task;
use crate::proto::status_streaming;
use crate::proto::status_streaming::vis_part::Geom;
use crate::proto::status_streaming::{HostAdvertisement, Status, VisAdvertisement};
use crate::state_filter::StateFilter;
use async_channel::{Receiver, Sender};
use bevy::mesh::{CylinderAnchor, CylinderMeshBuilder, SphereKind, SphereMeshBuilder};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use network_interface::NetworkInterface;
use std::cmp::PartialEq;
use std::collections::HashSet;
use std::net::{Ipv6Addr, SocketAddrV6};

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
                receive_vis_advertisements,
                receive_field_updates,
                handle_render_settings_change.run_if(resource_changed::<RenderSettings>),
            ),
            (
                update_game_state,
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
    discovery_channel: Receiver<Vec<((SocketAddrV6, Vec<NetworkInterface>), HostAdvertisement)>>,
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

#[derive(Resource, Debug)]
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

// ======== Field components ========

#[derive(Component, Debug)]
#[require(GameState, FieldGeometry, VisSelection, Transform)]
pub struct Field {
    pub host: FieldHost,
    update_task: FieldUpdateTask,
    state_filter: StateFilter,
}

impl Field {
    pub fn bind(host: FieldHost) -> Self {
        // The address is allocated according to https://datatracker.ietf.org/doc/html/rfc4607#section-1:
        // FF35::8000:xxxx
        // |  |  |    >>>> The port bound by the host for receiving data requests. This ensures that multiple hosts running on the same device don't interfere with each other.
        // |  |  >>>> Magic number from 8000 to FFFF. Might be used to separate different services with similar protocols to this one in the future
        // |  > Site-Local scope
        // >>> Required for source-specific multicast addresses
        // Hosts running on the same port on different devices may send to the same multicast group address, but their traffic can be differentiated by its source address.
        let multicast_address = Ipv6Addr::from_bits(
            0xFF35_0000_0000_0000_0000_0000_8000_0000 + host.addr.port() as u128,
        );

        let (state_sender, state_receiver) = async_channel::bounded(100);
        let state_rx_task = IoTaskPool::get().spawn(network_tasks::status_rx_task(
            state_sender,
            multicast_address,
            host.addr,
            host.interfaces.clone(),
        ));
        let (vis_available_sender, vis_available_receiver) = async_channel::bounded(5);
        let (vis_selected_sender, vis_selected_receiver) = async_channel::bounded(5);
        let vis_select_task = IoTaskPool::get().spawn(network_tasks::vis_select_task(
            vis_available_sender,
            vis_selected_receiver,
            multicast_address,
            host.addr,
            host.interfaces.clone(),
        ));

        debug!(
            "Spawned new field for host {}{}",
            host.addr,
            host.hostname
                .as_ref()
                .map(|name| format!(" ({name})"))
                .unwrap_or_default()
        );

        Field {
            host,
            update_task: FieldUpdateTask {
                vis_available_receiver,
                vis_selected_sender,
                vis_select_task,
                state_receiver,
                state_rx_task,
            },
            state_filter: StateFilter::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldHost {
    pub addr: SocketAddrV6,
    pub interfaces: Vec<u32>,
    pub hostname: Option<String>,
}

#[derive(Debug)]
struct FieldUpdateTask {
    vis_available_receiver: Receiver<VisAdvertisement>,
    vis_selected_sender: Sender<Vec<u32>>,
    vis_select_task: Task<()>,
    state_receiver: Receiver<Status>,
    state_rx_task: Task<()>,
}

#[derive(Component, Debug, Default, Clone, PartialEq, Eq)]
pub struct GameState {
    pub yellow_team: String,
    pub blue_team: String,
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct FieldGeometry {
    pub play_area_size: Vec2,
    pub boundary_width: f32,
    pub defense_size: Vec2,
    pub goal_width: f32,
}

impl FieldGeometry {
    pub fn div_a() -> Self {
        Self {
            play_area_size: Vec2::new(12.0, 9.0),
            boundary_width: 0.3,
            defense_size: Vec2::new(1.8, 3.6),
            goal_width: 1.8,
        }
    }

    pub fn div_b() -> Self {
        Self {
            play_area_size: Vec2::new(9.0, 6.0),
            boundary_width: 0.3,
            defense_size: Vec2::new(1.0, 2.0),
            goal_width: 1.0,
        }
    }
}

impl Default for FieldGeometry {
    fn default() -> Self {
        Self::div_a()
    }
}

#[derive(Component, Debug, Default)]
pub struct VisSelection {
    pub available: HashMap<u32, String>,
    pub selected: HashSet<u32>,
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
            // New task will be started next frame
        } else {
            // Handle new host list if available. There should only ever be one at a time.
            if let Ok(new_hosts) = discovery_task.discovery_channel.try_recv() {
                let new_hosts = new_hosts
                    .into_iter()
                    .map(|((addr, interfaces), host)| FieldHost {
                        addr,
                        interfaces: interfaces.iter().map(|i| i.index).collect(),
                        hostname: host.hostname,
                    })
                    .collect::<HashSet<_>>();

                // Only update the resource (and trigger change detection) when the hosts have actually changed
                if new_hosts != available_hosts.0 {
                    available_hosts.0 = new_hosts;
                }
            }
        }
    } else {
        // Start new discovery task
        let (tx, rx) = async_channel::bounded(5);
        let task = IoTaskPool::get().spawn(host_discovery_task(tx));
        commands.insert_resource(HostDiscoveryTask {
            discovery_channel: rx,
            discovery_task: task,
        });
        info!("Host discovery task started");
    }
}

fn receive_vis_advertisements(
    mut commands: Commands,
    mut q_fields: Query<(&Field, &mut VisSelection, Entity)>,
) {
    for (field, mut vis_selection, entity) in q_fields.iter_mut() {
        if field.update_task.vis_select_task.is_finished() {
            commands.entity(entity).despawn();
            return;
        }
        while let Ok(new_available) = field.update_task.vis_available_receiver.try_recv() {
            vis_selection.available.clear();
            new_available.visualization.into_iter().for_each(|vis| {
                vis_selection.available.insert(vis.id, vis.name);
            });
            _ = field
                .update_task
                .vis_selected_sender
                .try_send(vis_selection.selected.iter().copied().collect());
        }
    }
}

fn receive_field_updates(mut commands: Commands, mut q_fields: Query<(&mut Field, Entity)>) {
    for (mut field, entity) in q_fields.iter_mut() {
        if field.update_task.state_rx_task.is_finished() {
            commands.entity(entity).despawn();
            return;
        }
        while let Ok(new_status) = field.update_task.state_receiver.try_recv() {
            field.state_filter.push_packet(new_status);
        }
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

fn update_game_state(mut q_fields: Query<(&Field, &mut GameState)>) {
    for (field, mut game_state) in &mut q_fields {
        *game_state = field.state_filter.current_game_state();
    }
}

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
    mut q_fields: Query<(
        &Field,
        &mut FieldGeometry,
        Option<&FieldModelInstance>,
        Entity,
    )>,
) {
    for (field, mut field_geometry, modes_instance, entity) in &mut q_fields {
        let new_field_geom = field.state_filter.current_field_geometry();

        if render_settings.field && (*field_geometry != new_field_geom || modes_instance.is_none())
        {
            if let Some(instance) = modes_instance {
                scene_spawner.despawn_instance(instance.0);
            }
            let field_model = field_mesh(&new_field_geom, &mut mesh_assets, &mut material_assets);
            commands.entity(entity).insert(FieldModelInstance(
                scene_spawner.spawn_as_child(scene_assets.add(field_model), entity),
            ));
            *field_geometry = new_field_geom;
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
        Query<(&Field, Entity)>,
        Query<(&Robot, &Team, &mut Transform, &ChildOf, Entity)>,
        Query<(&Transform, &ChildOf, Entity), (With<Ball>, Without<Robot>)>,
    ),
) {
    for (field, field_entity) in &q_fields {
        let world_state = field.state_filter.current_world_state();

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

        let mut update_robots = |team: Team, new_robots: Vec<status_streaming::Robot>| {
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
    // TODO: Replace with AssetId (weak handle)
    pub circle: HashMap<u32, Handle<Mesh>>,
    pub polygon: HashMap<Vec<(i32, i32)>, Handle<Mesh>>,
    pub path: HashMap<Vec<(i32, i32)>, Handle<Mesh>>,
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
    (q_fields, q_visualizations): (
        Query<(&Field, &VisSelection, Entity)>,
        Query<(&Visualization, &ChildOf, Entity)>,
    ),
) {
    for (field, vis_names, field_entity) in &q_fields {
        let (group_count, updated_groups, new_visualizations) =
            field.state_filter.visualization_updates();

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
                                (handle.clone(), Vec2::new(c.p_x, c.p_y))
                            } else {
                                let new_handle =
                                    mesh_assets.add(circle_visualization_mesh(32, c.radius));
                                vis_models.circle.insert(mm_radius, new_handle.clone());
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
                                (handle.clone(), s)
                            } else {
                                let new_handle =
                                    mesh_assets.add(polygon_visualization_mesh(&points));
                                vis_models.polygon.insert(mm_points, new_handle.clone());
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
                            if let Some(handle) = vis_models.path.get(&mm_points) {
                                (handle.clone(), s)
                            } else {
                                let new_handle =
                                    mesh_assets.add(path_visualization_mesh(&points, 0.01));
                                vis_models.path.insert(mm_points, new_handle.clone());
                                (new_handle, s)
                            }
                        }
                        None => {
                            warn!(
                                "Invalid visualization part in {}: No geometry",
                                vis_names
                                    .available
                                    .get(&visualization.id)
                                    .unwrap_or(&visualization.id.to_string())
                            );
                            continue;
                        }
                        _ => {
                            warn!(
                                "Invalid visualization part in {}: Empty geometry",
                                vis_names
                                    .available
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

        // Forget unused meshes that were unloaded by the bevy asset system
        vis_models
            .circle
            .retain(|_, asset_id| mesh_assets.contains(asset_id));
        vis_models
            .polygon
            .retain(|_, asset_id| mesh_assets.contains(asset_id));
        vis_models
            .path
            .retain(|_, asset_id| mesh_assets.contains(asset_id));
    }
}
