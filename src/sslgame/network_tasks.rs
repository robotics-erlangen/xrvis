use super::proto::status_streaming::{DataRequest, HostAdvertisement, Status, VisAdvertisement};
use super::ssm_socket::SSMSocketExtension;
use async_channel::{Receiver, Sender, TrySendError};
use async_net::UdpSocket;
use bevy::prelude::*;
use bevy::tasks::futures_lite::FutureExt;
use prost::Message;
use std::net::{IpAddr, Ipv6Addr, SocketAddrV6};
use std::time::{Duration, Instant};

// TODO: Leave multicast groups before stopping

const HOST_DISCOVERY_ADDR: Ipv6Addr =
    Ipv6Addr::from_bits(0xFF15_0000_0000_0000_0045_5246_6F72_6365); // "ERForce" in hex
const HOST_DISCOVERY_PORT: u16 = 11000;
const DATA_PORT: u16 = 11001;
const VIS_AD_PORT: u16 = 11002;

pub async fn host_discovery_task(host_list_out: Sender<Vec<(Ipv6Addr, HostAdvertisement)>>) {
    let host_advert_socket =
        UdpSocket::bind_multicast((Ipv6Addr::UNSPECIFIED, HOST_DISCOVERY_PORT)).unwrap();
    host_advert_socket
        .join_multicast_v6(&HOST_DISCOVERY_ADDR, 0)
        .unwrap();

    let mut rx_buf = [0u8; 1024]; // Small message, so 1kb should be enough

    let mut next_time = Instant::now();
    loop {
        let mut hosts: Vec<(Ipv6Addr, HostAdvertisement)> = Vec::new();

        // Collect all unique packets received within a three-second window
        next_time += Duration::from_secs(3);
        while let Ok((size, new_source)) = host_advert_socket
            .recv_from(&mut rx_buf)
            .or(async {
                async_io::Timer::at(next_time).await;
                Err(std::io::ErrorKind::TimedOut.into())
            })
            .await
        {
            let IpAddr::V6(new_source) = new_source.ip() else {
                continue;
            };
            let Ok(new_host) = HostAdvertisement::decode(&rx_buf[..size]) else {
                continue;
            };
            if hosts.iter().all(|(source, host)| {
                !(*source == new_source && host.instance_port == new_host.instance_port)
            }) {
                hosts.push((new_source, new_host));
            }
        }

        match host_list_out.try_send(hosts) {
            Err(TrySendError::Full(_)) => warn!("Host list channel full"),
            Err(TrySendError::Closed(_)) => return,
            _ => {}
        }
    }
}

pub async fn vis_select_task(
    available_out: Sender<VisAdvertisement>,
    selected_in: Receiver<Vec<u32>>,
    mcast_addr: Ipv6Addr,
    source: SocketAddrV6,
) {
    let mut rx_buf = [0u8; 65535]; // Max size of an udp datagram
    let rx_socket = UdpSocket::bind_multicast((Ipv6Addr::UNSPECIFIED, VIS_AD_PORT)).unwrap();
    rx_socket.join_ssm_v6(mcast_addr, *source.ip(), 0).unwrap();

    let mut selected_vis = DataRequest::default();
    let tx_socket = UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 0)).await.unwrap();

    while let Ok(size) = rx_socket.recv(&mut rx_buf).await {
        let available_vis = VisAdvertisement::decode(&rx_buf[..size]).unwrap();

        match available_out.try_send(available_vis) {
            Err(TrySendError::Full(_)) => warn!("Part rx channel full"),
            Err(TrySendError::Closed(_)) => return,
            _ => {}
        }

        if let Ok(new_selected) = selected_in.try_recv() {
            selected_vis.visualization_id.clear();
            new_selected
                .into_iter()
                .for_each(|vis| selected_vis.visualization_id.push(vis));
        }
        let mut tx_buf = vec![0u8; selected_vis.encoded_len()];
        selected_vis.encode(&mut tx_buf).unwrap();
        tx_socket.send_to(&tx_buf, source).await.unwrap();
    }
}

pub async fn status_rx_task(
    status_out: Sender<Status>,
    mcast_addr: Ipv6Addr,
    source_addr: Ipv6Addr,
) {
    let mut rx_buf = [0u8; 65535]; // Max size of an udp datagram
    let rx_socket = UdpSocket::bind_multicast((Ipv6Addr::UNSPECIFIED, DATA_PORT)).unwrap();
    rx_socket.join_ssm_v6(mcast_addr, source_addr, 0).unwrap();

    let mut warn_timeout = Instant::now();

    while let Ok(size) = rx_socket.recv(&mut rx_buf).await {
        let status = Status::decode(&rx_buf[..size]).unwrap();

        match status_out.try_send(status) {
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
