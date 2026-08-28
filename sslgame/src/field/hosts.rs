use crate::field::game_state::GameState;
use crate::field::geometry::FieldGeometry;
use crate::field::robots::update_world_state;
use crate::field::visualizations::{update_visualization_names, update_visualizations};
use crate::network_tasks;
use crate::network_tasks::UpdatePacket;
use crate::proto::remote::udp_stream_request::UdpStream;
use crate::proto::remote::ws_stream_request::WsStream;
use crate::proto::remote::{UdpStreamRequest, WsStreamRequest, ws_request};
use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use std::net::SocketAddr;
use std::time::Instant;

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct UpdateHostDataSystemSet;

/// The core plugin for handling network traffic from hosts.
/// It receives incoming pakets from active [HostConnection]s and encodes them into the ecs.
pub fn host_plugin(app: &mut App) {
    app.add_systems(
        PreUpdate,
        receive_host_packets.in_set(UpdateHostDataSystemSet),
    );
}

/// Representation of a discovered host. Note that this component alone does not mean that
/// the host is connected, just that it was found in discovery: Use [start_connection](Self::start_connection)
/// to actually establish the connection and start tracking its field state.
#[derive(Component, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Host {
    pub websocket_addr: SocketAddr,
    pub hostname: Option<String>,
}

impl Host {
    /// Starts a background IO task for talking with a host. Add the returned [HostConnection]
    /// component to the [Host] entity to start receiving and tracking its state.
    /// Create a [Field] with host relations (like [YellowRobotHost]) to actually use this host's data.
    pub fn start_connection(&self) -> HostConnection {
        let (rx_sender, rx_receiver) = async_channel::bounded(100);
        let (tx_sender, tx_receiver) = async_channel::bounded(10);
        let state_rx_task = IoTaskPool::get().spawn(network_tasks::io_task(
            self.websocket_addr,
            rx_sender,
            tx_receiver,
        ));

        debug!(
            "Established connection to host {}{}",
            self.websocket_addr,
            self.hostname
                .as_ref()
                .map(|name| format!(" ({name})"))
                .unwrap_or_default()
        );

        // Request all data streams by default
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

        HostConnection {
            sender: tx_sender,
            receiver: rx_receiver,
            io_task: state_rx_task,
        }
    }
}

impl std::fmt::Display for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(hostname) = &self.hostname {
            write!(f, "{}:{}", hostname, self.websocket_addr.port())
        } else {
            write!(f, "{}", self.websocket_addr)
        }
    }
}

/// Handle to the IO task for an actively connected host. Create using [Host::start_connection], then insert
/// it into the Host's entity to start updating related fields ([GeometryHost], [YellowRobotHost], ...).
#[derive(Component, Debug)]
pub struct HostConnection {
    pub sender: Sender<ws_request::Content>,
    receiver: Receiver<(UpdatePacket, Instant)>,
    io_task: Task<()>,
}

// ======== Relationships ========

// These relationships dictate where each field is getting its data from and use
// linked spawn to automatically despawn related fields when the host is dropped.

/// Can be added to a [Field](crate::field::Field) to receive a 3d model based on the [FieldGeometry] of that host.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
#[relationship(relationship_target = GeometryFields)]
pub struct GeometryHost(pub Entity);
#[derive(Component, Debug, Clone, PartialEq, Eq)]
#[relationship_target(relationship = GeometryHost, linked_spawn)]
pub struct GeometryFields(Vec<Entity>);

/// Can be added to a [Field](crate::field::Field) to receive [GameState] from that host.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
#[relationship(relationship_target = GameStateFields)]
pub struct GameStateHost(pub Entity);
#[derive(Component, Debug, Clone, PartialEq, Eq)]
#[relationship_target(relationship = GameStateHost, linked_spawn)]
pub struct GameStateFields(Vec<Entity>);

/// Can be added to a [Field](crate::field::Field) to receive a [Ball](crate::field::robots::Ball) child entity, taking its position from that host.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
#[relationship(relationship_target = BallFields)]
pub struct BallHost(pub Entity);
#[derive(Component, Debug, Clone, PartialEq, Eq)]
#[relationship_target(relationship = BallHost, linked_spawn)]
pub struct BallFields(Vec<Entity>);

/// Can be added to a [Field](crate::field::Field) to receive [Robot](crate::field::robots::Robot) child entities for all
/// yellow robots from that host. Their rendering can be configured using [RenderSettings](crate::RenderSettings).
#[derive(Component, Debug, Clone, PartialEq, Eq)]
#[relationship(relationship_target = YellowRobotFields)]
pub struct YellowRobotHost(pub Entity);
#[derive(Component, Debug, Clone, PartialEq, Eq)]
#[relationship_target(relationship = YellowRobotHost, linked_spawn)]
pub struct YellowRobotFields(Vec<Entity>);

/// Can be added to a [Field](crate::field::Field) to receive [Robot](crate::field::robots::Robot) child entities for all
/// blue robots from that host. Their rendering can be configured using [RenderSettings](crate::RenderSettings).
#[derive(Component, Debug, Clone, PartialEq, Eq)]
#[relationship(relationship_target = BlueRobotFields)]
pub struct BlueRobotHost(pub Entity);
#[derive(Component, Debug, Clone, PartialEq, Eq)]
#[relationship_target(relationship = BlueRobotHost, linked_spawn)]
pub struct BlueRobotFields(Vec<Entity>);

#[allow(clippy::type_complexity)]
fn receive_host_packets(
    mut commands: Commands,
    mut q_hosts: Query<(
        &Host,
        &HostConnection,
        Option<&mut FieldGeometry>,
        Option<&mut GameState>,
        Entity,
    )>,
) {
    for (host, host_conn, mut geom, mut game_state, host_entity) in q_hosts.iter_mut() {
        if host_conn.io_task.is_finished() {
            info!(
                "Connection to {} closed, despawning all related fields",
                host.websocket_addr
            );
            commands.entity(host_entity).despawn();
            continue;
        }
        while let Ok((packet, rx_time)) = host_conn.receiver.try_recv() {
            // The host should only send geom and game state update when they actually changed, but its still safer to check ourselves
            match packet {
                UpdatePacket::FieldGeom(new_geom) => {
                    if let Some(geom) = geom.as_mut() {
                        geom.set_if_neq(FieldGeometry::from(new_geom));
                    } else {
                        warn!(
                            "Discarding geometry update from host {host} because it has no FieldGeometry component. Did you forget to add the field_geometry_plugin?"
                        );
                    }
                }
                UpdatePacket::GameState(new_game_state) => {
                    if let Some(game_state) = game_state.as_mut() {
                        game_state.set_if_neq(GameState(new_game_state));
                    } else {
                        warn!(
                            "Discarding game state update from host {host} because it has no GameState component. Did you forget to add the game_state_plugin?"
                        );
                    }
                }
                UpdatePacket::VisMappings(new_vis_mappings) => {
                    commands.run_system_cached_with(
                        update_visualization_names,
                        (host_entity, new_vis_mappings),
                    );
                }
                UpdatePacket::WorldState(new_world_state) => {
                    // TODO: Currently the world state is the only thing that is directly written to the fields.
                    // All other data has a separate system for transferring host -> field, so this feels inconsistent.
                    commands.run_system_cached_with(
                        update_world_state,
                        (host_entity, new_world_state, rx_time),
                    );
                }
                UpdatePacket::VisualizationUpdate(vis_update) => {
                    commands
                        .run_system_cached_with(update_visualizations, (host_entity, vis_update));
                }
            }
        }
    }
}
