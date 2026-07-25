pub mod discovery;
pub mod robots;
pub mod visualizations;

use crate::field::discovery::AvailableHosts;
use crate::field::visualizations::SelectedVisualizations;
use crate::mesh_gen::field::field_mesh;
use crate::network_tasks::UpdatePacket;
use crate::proto::remote::udp_stream_request::UdpStream;
use crate::proto::remote::ws_stream_request::WsStream;
use crate::proto::remote::{UdpStreamRequest, WsStreamRequest, ws_request};
use crate::visualization_tracker::VisualizationTracker;
use crate::{DefaultMaterial, RenderSettings, network_tasks, proto};
use async_channel::{Receiver, Sender};
use bevy::math::Vec2;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use std::net::SocketAddr;
use std::time::Instant;
use tracing::{debug, info};
use visualizations::AvailableVisualizations;

/// Marker component for field elements
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    #[default]
    Yellow,
    Blue,
}

pub fn field_plugin(app: &mut App) {
    app.add_plugins(discovery::field_discovery_plugin);
    app.add_plugins(robots::field_robot_plugin);
    app.add_plugins(visualizations::field_vis_plugin);

    app.add_systems(PreUpdate, receive_field_updates);
    app.add_systems(Update, update_field_geometry);
}

#[derive(Component, Debug)]
#[require(
    Visibility,
    Transform,
    GameState,
    FieldGeometry,
    AvailableVisualizations,
    SelectedVisualizations,
    VisualizationTracker
)]
pub struct Field {
    pub host: FieldHost,
    pub connection: FieldConnection,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldHost {
    pub websocket_addr: SocketAddr,
    pub hostname: Option<String>,
}

#[derive(Debug)]
pub struct FieldConnection {
    pub sender: Sender<ws_request::Content>,
    receiver: Receiver<(UpdatePacket, Instant)>,
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
                // Port will be set in the io task
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

#[derive(Component, Deref, Debug, Default, Clone, PartialEq, Eq)]
pub struct GameState(proto::remote::GameState);

#[derive(Component, Debug, Clone, PartialEq)]
pub struct FieldGeometry {
    pub play_area_size: Vec2,
    pub boundary_width: f32,
    pub defense_size: Vec2,
    pub goal_width: f32,
}

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

fn receive_field_updates(
    mut commands: Commands,
    mut available_hosts: ResMut<AvailableHosts>,
    mut q_fields: Query<(
        &Field,
        &mut FieldGeometry,
        &mut GameState,
        &mut AvailableVisualizations,
        &mut VisualizationTracker,
        Entity,
    )>,
) {
    for (field, mut geom, mut game_state, mut vis_selection, mut vis_tracker, entity) in
        q_fields.iter_mut()
    {
        if field.connection.io_task.is_finished() {
            info!(
                "Connection to {} closed, despawning field entities",
                field.host.websocket_addr
            );
            available_hosts.dropped.insert(field.host.clone());
            commands.entity(entity).despawn();
            continue;
        }
        while let Ok((new_packet, rx_time)) = field.connection.receiver.try_recv() {
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
                    game_state.set_if_neq(GameState(new_game_state));
                }
                UpdatePacket::VisMappings(new_vis_mappings) => {
                    vis_selection.sources = new_vis_mappings.source;
                    vis_selection.visualizations = new_vis_mappings.name;
                }
                UpdatePacket::WorldState(new_world_state) => {
                    commands.run_system_cached_with(
                        robots::update_world_state,
                        (entity, new_world_state, rx_time),
                    );
                }
                UpdatePacket::VisualizationUpdate(vis_update) => {
                    vis_tracker.push_update(vis_update);
                }
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_field_geometry(
    mut commands: Commands,
    render_settings: Res<RenderSettings>,
    white_material: Res<DefaultMaterial>,
    mut mesh_assets: ResMut<Assets<Mesh>>,
    mut q_fields: Query<(Ref<FieldGeometry>, Option<&Mesh3d>, Entity)>,
) {
    for (field_geometry, mesh_component, entity) in &mut q_fields {
        if render_settings.field && (field_geometry.is_changed() || mesh_component.is_none()) {
            commands.entity(entity).insert((
                Mesh3d(mesh_assets.add(field_mesh(&field_geometry))),
                MeshMaterial3d(white_material.opaque.clone()),
            ));
        }
    }
}
