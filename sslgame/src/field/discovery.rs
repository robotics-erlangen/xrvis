use crate::field::FieldHost;
use crate::network_tasks::host_discovery_task;
use crate::proto::remote::HostAdvertisement;
use async_channel::Receiver;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use std::collections::HashSet;
use std::net::SocketAddr;
use tracing::{error, info};

pub fn field_discovery_plugin(app: &mut App) {
    app.insert_resource(AvailableHosts::default());
    app.add_systems(PreUpdate, receive_host_advertisements);
}

#[derive(Resource, Debug, Default)]
pub struct AvailableHosts {
    pub(crate) discovered: HashSet<FieldHost>,
    /// All hosts that were dropped because of websocket exits since the last discovery update.\
    /// This separation is necessary because a field might be despawned just after the discovery received
    /// its advertisement for this cycle, causing it to still be included in the next update.
    /// The `dropped` list can be used to filter for these zombie connections and also
    /// communicate the disconnect to users immediately, before the next discovery update
    /// (changes to `dropped` also trigger change detection).
    pub(crate) dropped: HashSet<FieldHost>,
}

impl AvailableHosts {
    /// Gets all currently available hosts, correctly handling all types of disconnects.
    pub fn available(&self) -> impl Iterator<Item = &FieldHost> {
        self.discovered.iter().filter(|h| !self.dropped.contains(h))
    }
}

#[derive(Resource, Debug)]
struct HostDiscoveryTask {
    discovery_channel: Receiver<Vec<(SocketAddr, HostAdvertisement)>>,
    discovery_task: Task<()>,
}

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
                    // Skip any hosts that might have been dropped after their advertisement was already received in this discovery cycle.
                    .filter(|h| !available_hosts.dropped.contains(h))
                    .collect::<HashSet<_>>();

                // The dropped list can't affect the next discovery, so it can be cleared.
                // Only mut deref the resource (and trigger change detection) when the hosts have actually changed
                if !available_hosts.dropped.is_empty() {
                    available_hosts.dropped.clear();
                }

                if new_hosts != available_hosts.discovered {
                    available_hosts.discovered = new_hosts;
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
