use crate::field::hosts::Host;
use crate::network_tasks::host_discovery_task;
use crate::proto::remote::HostAdvertisement;
use async_channel::{Receiver, Sender};
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use std::net::SocketAddr;
use tracing::{error, info};

/// Handles automatic creation and removal of [Host] entities
pub fn discovery_plugin(app: &mut App) {
    app.add_systems(PreUpdate, receive_host_advertisements);
    app.world_mut()
        .register_component_hooks::<Host>()
        .on_remove(on_host_dropped);
}

/// Handle to the currently running discovery task
#[derive(Resource, Debug)]
struct HostDiscoveryTask {
    discovery_channel: Receiver<Vec<(SocketAddr, HostAdvertisement)>>,
    drop_feedback_channel: Sender<SocketAddr>,
    discovery_task: Task<()>,
}

/// Forwards the dropped host to the discovery task so it can be immediately removed from any remaining internal state.
/// Without this, the dropped host could still be included in the next host list if that is sent before its timeout.
fn on_host_dropped(mut world: DeferredWorld, context: HookContext) {
    let dropped_host_addr = world.get::<Host>(context.entity).unwrap().websocket_addr;
    let discover_task = world.resource_mut::<HostDiscoveryTask>();
    discover_task
        .drop_feedback_channel
        .send_blocking(dropped_host_addr)
        .unwrap();
}

/// Manages the HostDiscoveryTask and updates the AvailableHosts resource
fn receive_host_advertisements(
    mut commands: Commands,
    running_receiver: Option<Res<HostDiscoveryTask>>,
    q_available_hosts: Query<(&Host, Entity)>,
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
                        Host {
                            websocket_addr,
                            hostname: adv.hostname,
                        }
                    })
                    .collect::<Vec<_>>();
                let current_hosts = q_available_hosts.iter().collect::<Vec<_>>();

                // Remove old hosts
                current_hosts
                    .iter()
                    .filter(|(ch, _)| {
                        !new_hosts
                            .iter()
                            .any(|nh| nh.websocket_addr == ch.websocket_addr)
                    })
                    .for_each(|(_, e)| {
                        commands.entity(*e).despawn();
                    });
                // Add new hosts
                new_hosts
                    .into_iter()
                    .filter(|nh| {
                        !current_hosts
                            .iter()
                            .any(|(ch, _)| ch.websocket_addr == nh.websocket_addr)
                    })
                    .for_each(|nh| {
                        commands.spawn(nh);
                    });
            }
        }
    } else {
        // Start a new discovery task
        let (host_tx, host_rx) = async_channel::bounded(5);
        let (dropped_tx, dropped_rx) = async_channel::bounded(5);
        let task = IoTaskPool::get().spawn(host_discovery_task(host_tx, dropped_rx));
        commands.insert_resource(HostDiscoveryTask {
            discovery_channel: host_rx,
            drop_feedback_channel: dropped_tx,
            discovery_task: task,
        });
        info!("Host discovery task started");
    }
}
