use super::proto::status_streaming::{DataRequest, HostAdvertisement, Status, VisAdvertisement};
use crate::sslgame::networking::interface_flags::NetworkInterfaceFlagExtension;
use crate::sslgame::networking::ssm_socket::SSMSocketExtension;
use async_channel::{Receiver, Sender, TrySendError};
use async_net::UdpSocket;
use bevy::prelude::*;
use bevy::tasks::futures_lite::{StreamExt, stream};
use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use prost::Message;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6};
use std::time::{Duration, Instant};

// TODO: Leave multicast groups before stopping

const HOST_DISCOVERY_ADDR: Ipv6Addr =
    Ipv6Addr::from_bits(0xFF15_0000_0000_0000_0045_5246_6F72_6365); // "ERForce" in hex
const HOST_DISCOVERY_PORT: u16 = 11000;
const DATA_PORT: u16 = 11001;
const VIS_AD_PORT: u16 = 11002;

pub async fn host_discovery_task(
    host_list_out: Sender<Vec<((SocketAddrV6, NetworkInterface), HostAdvertisement)>>,
) {
    // Listen on all interfaces by creating a new socket for each one.
    // This is necessary because getting the origin interface of a packet is very difficult on windows (see https://github.com/rust-lang/socket2/pull/447)
    let mut sockets: Vec<(UdpSocket, NetworkInterface)> = Vec::new();

    // Collect all unique packets received in three-second windows
    let mut next_time = Instant::now();
    loop {
        next_time += Duration::from_secs(3);

        // Create sockets for new interfaces
        if let Ok(if_list) = NetworkInterface::show() {
            // Drop all sockets for removed interfaces
            sockets.retain(|(_, olf_if)| !if_list.contains(olf_if));

            // Create new sockets for new interfaces
            let new_interfaces = if_list
                .into_iter()
                .filter(|new_if| new_if.is_multicast() && new_if.is_up())
                .filter(|new_if| !sockets.iter().any(|(_, old_if)| old_if == new_if))
                .collect::<Vec<_>>();
            new_interfaces
                .into_iter()
                .filter(|new_if| new_if.addr.iter().any(|a| a.ip().is_ipv6()))
                .for_each(|new_if| {
                    let new_sock =
                        UdpSocket::bind_multicast((Ipv6Addr::UNSPECIFIED, HOST_DISCOVERY_PORT))
                            .unwrap();
                    if new_sock
                        .join_multicast_v6(&HOST_DISCOVERY_ADDR, new_if.index)
                        .is_ok()
                    {
                        sockets.push((new_sock, new_if));
                    }
                });
        }

        // Create packet stream from all sockets.
        // This stream returns the packets received by any socket and an io::ErrorKind::TimedOut error after a set timeout.
        // The streams are merged with StreamExt::or, which is biased towards the first stream.
        // This ensures that the timeout will always be returned right away and can't be blocked by a socket overloading the stream with too many packets.
        let mut packet_stream = stream::once_future(async {
            async_io::Timer::at(next_time).await;
            Err(std::io::ErrorKind::TimedOut.into())
        })
        .boxed();
        let individual_streams = sockets.iter().map(|(sock, sock_if)| {
            stream::unfold(sock, async |sock| {
                let mut rx_buf = [0u8; 1024]; // Small message, so 1kb should be enough
                let result = sock
                    .recv_from(&mut rx_buf)
                    .await
                    .map(|(size, source_addr)| (size, source_addr, sock_if.clone(), rx_buf));
                Some((result, sock))
            })
        });
        for stream in individual_streams {
            packet_stream = packet_stream.or(stream).boxed()
        }

        let mut hosts: Vec<((SocketAddrV6, NetworkInterface), HostAdvertisement)> = Vec::new();

        // Collect packets from the merged stream until an error is encountered (the timeout also returns an error)
        while let Some(Ok((size, source_addr, source_if, rx_buf))) = packet_stream.next().await {
            let SocketAddr::V6(new_source_addr) = source_addr else {
                continue;
            };
            let Ok(new_host) = HostAdvertisement::decode(&rx_buf[..size]) else {
                debug!("Invalid host advertisement received from {new_source_addr}");
                continue;
            };

            // When the same host advert is received on multiple interfaces, one must be chosen deterministically to prevent flickering.
            // The lowest interface with the lowest index is used because indices are given out in ascending order
            // and more "native"/local interfaces like loopback tend to be registered first.
            if hosts
                .iter()
                .any(|((a, i), _)| *a == new_source_addr && source_if.index < i.index)
            {
                // Received packet for a known host on a lower interface index -> Replace old host entry
                hosts.retain(|((a, _), _)| *a != new_source_addr);
                hosts.push(((new_source_addr, source_if), new_host));
            } else if !hosts.iter().any(|((a, _), info)| {
                (new_host.hostname.is_some()
                    && info.hostname == new_host.hostname
                    && a.port() == new_source_addr.port())
                    || *a == new_source_addr
            }) {
                // Received packet from previously unknown host -> Add to list
                hosts.push(((new_source_addr, source_if), new_host));
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
    if_index: u32,
) {
    let mut rx_buf = [0u8; 65535]; // Max size of an udp datagram
    let rx_socket = UdpSocket::bind_multicast((Ipv6Addr::UNSPECIFIED, VIS_AD_PORT)).unwrap();
    rx_socket
        .join_ssm_v6(mcast_addr, *source.ip(), if_index)
        .unwrap();

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
    if_index: u32,
) {
    let mut rx_buf = [0u8; 65535]; // Max size of an udp datagram
    let rx_socket = UdpSocket::bind_multicast((Ipv6Addr::UNSPECIFIED, DATA_PORT)).unwrap();
    rx_socket
        .join_ssm_v6(mcast_addr, source_addr, if_index)
        .unwrap();

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
