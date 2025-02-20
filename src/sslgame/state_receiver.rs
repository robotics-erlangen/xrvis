use crate::sslgame::proto::amun_compact::Status;
use async_channel::{Receiver, Sender, TrySendError};
use async_net::UdpSocket;
use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use prost::Message;
use std::net::Ipv6Addr;
use std::time::{Duration, Instant};

static STREAMING_ADDR: Ipv6Addr = Ipv6Addr::from_bits(0xFF15_0000_0000_0000_0045_5246_6F72_6365);

#[derive(Resource)]
pub struct StatusUpdateReceiver {
    pub channel: Receiver<Status>,
    pub task: Task<()>,
}

pub fn manage_rx_task(mut commands: Commands, running_receiver: Option<Res<StatusUpdateReceiver>>) {
    if let Some(r) = running_receiver {
        if r.task.is_finished() {
            commands.remove_resource::<StatusUpdateReceiver>();
            error!("Status rx task stopped");
            // New task will be started next frame
        }
    } else {
        // Start new rx task
        let (tx, rx) = async_channel::bounded(100);
        let task = IoTaskPool::get().spawn(status_rx_task(tx));
        commands.insert_resource(StatusUpdateReceiver { channel: rx, task });
        info!("Status rx task started");
    }
}

pub async fn status_rx_task(sender: Sender<Status>) {
    let mut rx_buf = [0u8; 65535]; // Max size of an udp datagram
    let socket = UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 8080))
        .await
        .unwrap(); // TODO: choose new port
    socket.join_multicast_v6(&STREAMING_ADDR, 0).unwrap();

    let mut warn_timeout = Instant::now();

    while let Ok(size) = socket.recv(&mut rx_buf).await {
        let status = Status::decode(&rx_buf[..size]).unwrap();
        //println!("Status received");

        match sender.try_send(status) {
            Err(TrySendError::Full(_)) => {
                if warn_timeout < Instant::now() {
                    warn!("Status rx channel full (system can't keep up)");
                    warn_timeout = Instant::now() + Duration::from_secs(5);
                }
            }
            Err(TrySendError::Closed(_)) => return,
            _ => {}
        }
    }
}
