use crate::proto::remote::*;
use async_channel::{Receiver, Sender, TrySendError};
use async_net::UdpSocket;
use async_tungstenite::tungstenite;
use bevy::prelude::*;
use bevy::tasks::futures_lite::{FutureExt, StreamExt, stream};
use bytes::BytesMut;
use net_ext::interface_flags::NetworkInterfaceFlagExtension;
use net_ext::ssm_socket::SSMSocketExtension;
use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use prost::Message;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::time::{Duration, Instant};

// TODO: Leave multicast groups before stopping

const BEACON_ADDR_V4: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(239, 1, 1, 1), 11000);
const BEACON_ADDR_V6: SocketAddrV6 = SocketAddrV6::new(
    Ipv6Addr::from_bits(0xFF15_0000_0000_0045_5246_6F72_6365_0001), // "ERForce" in hex
    11000,
    0,
    0,
);

#[derive(PartialEq, Eq, Hash)]
enum HostKey {
    Addr(SocketAddr),
    Id(u32),
}

pub async fn host_discovery_task(hosts_out: Sender<Vec<(SocketAddr, HostAdvertisement)>>) {
    let socket_v4 = UdpSocket::bind_multicast((Ipv4Addr::UNSPECIFIED, BEACON_ADDR_V4.port()))
        .expect("Failed to bind ipv4 discovery socket");
    let socket_v6 = UdpSocket::bind_multicast((Ipv6Addr::UNSPECIFIED, BEACON_ADDR_V6.port()))
        .expect("Failed to bind ipv6 discovery socket");

    let mut host_map: HashMap<_, (_, _, _)> = HashMap::new();

    // Forward discovery packets and check for new network interfaces every 3 seconds
    let mut active_interfaces = Vec::new();
    let mut next_interface_refresh = Instant::now();
    loop {
        next_interface_refresh += Duration::from_secs(3);
        // Forget old hosts
        {
            let cutoff = Instant::now() - Duration::from_secs(3);
            host_map.retain(|_, (t, _, _)| *t > cutoff);
        }

        // ======== Update multicast subscriptions ========

        match NetworkInterface::show() {
            Ok(if_list) => {
                // Get all relevant interfaces
                let filtered_if_list: Vec<_> = if_list
                    .into_iter()
                    .filter(|new_if| new_if.is_multicast() && new_if.is_up())
                    .collect();

                // Subscribe on new interfaces
                filtered_if_list
                    .iter()
                    .filter(|i| !active_interfaces.contains(&i.index))
                    .for_each(|new_if| {
                        if let Some(network_interface::Addr::V4(addr)) =
                            new_if.addr.iter().find(|a| a.ip().is_ipv4())
                        {
                            _ = socket_v4.join_multicast_v4(*BEACON_ADDR_V4.ip(), addr.ip);
                        } else if new_if.addr.iter().any(|a| a.ip().is_ipv6()) {
                            _ = socket_v6.join_multicast_v6(BEACON_ADDR_V6.ip(), new_if.index);
                        }
                    });

                active_interfaces = filtered_if_list.into_iter().map(|i| i.index).collect();
            }
            Err(e) => {
                error!("Failed to get network interface list, skipping interface update: {e}");
            }
        }

        // ======== Merge packet streams ========

        fn make_packet_stream(
            socket: &UdpSocket,
        ) -> impl stream::Stream<Item = io::Result<(usize, SocketAddr, [u8; 256])>> + '_ {
            // Hack to generate a packet stream from an udp socket. The socket is passed along as state.
            stream::unfold(socket, async |socket| {
                let mut rx_buf = [0u8; 256]; // Discovery packets are very small
                let result = socket
                    .recv_from(&mut rx_buf)
                    .await
                    .map(|(size, source_addr)| (size, source_addr, rx_buf));
                Some((result, socket))
            })
        }

        let stream_v4 = make_packet_stream(&socket_v4);
        let stream_v6 = make_packet_stream(&socket_v6);
        let stream_timeout = stream::once_future(async {
            async_io::Timer::at(next_interface_refresh).await;
            Err(io::ErrorKind::TimedOut.into())
        });

        let mut merged_stream = stream_v4.or(stream_v6).or(stream_timeout).boxed();

        // ======== Collect packets from the merged stream until the timeout ========

        loop {
            match merged_stream
                .next()
                .await
                .expect("The host discovery stream should never yield None")
            {
                Ok((size, source_addr, rx_buf)) => {
                    let new_host = match HostAdvertisement::decode(&rx_buf[..size]) {
                        Ok(host) => {
                            debug!("Received host advertisement from {source_addr}");
                            host
                        }
                        Err(e) => {
                            warn!("Invalid host advertisement received from {source_addr}: {e}");
                            continue;
                        }
                    };

                    if let Some(instance_id) = new_host.instance_id {
                        match host_map.entry(HostKey::Id(instance_id)) {
                            Entry::Occupied(mut entry) => entry.get_mut().0 = Instant::now(),
                            Entry::Vacant(entry) => {
                                entry.insert((Instant::now(), source_addr, new_host));
                            }
                        }
                    } else {
                        match host_map.entry(HostKey::Addr(source_addr)) {
                            Entry::Occupied(mut entry) => entry.get_mut().0 = Instant::now(),
                            Entry::Vacant(entry) => {
                                entry.insert((Instant::now(), source_addr, new_host));
                            }
                        }
                    }

                    let host_list: Vec<_> = host_map
                        .iter()
                        .map(|(_, (_, a, h))| (*a, h.clone()))
                        .collect();

                    match hosts_out.try_send(host_list) {
                        Ok(_) => {}
                        Err(TrySendError::Full(_)) => warn!("Host discovery channel full"),
                        Err(TrySendError::Closed(_)) => {
                            info!("Host discovery channel dropped, stopping discovery task");
                            return;
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::TimedOut => break,
                Err(e) => {
                    error!("Host discovery network error, stopping discovery task: {e}");
                    return;
                }
            }
        }
    }
}

/// Combination of the WsPacket and UdpPacket protobuf messages
pub enum UpdatePacket {
    FieldGeom(FieldGeometry),
    GameState(GameState),
    VisMappings(VisMappings),
    WorldState(WorldState),
    VisualizationUpdate(VisualizationUpdate),
}

impl From<ws_packet::Content> for UpdatePacket {
    fn from(packet: ws_packet::Content) -> Self {
        match packet {
            ws_packet::Content::Geom(inner) => Self::FieldGeom(inner),
            ws_packet::Content::GameState(inner) => Self::GameState(inner),
            ws_packet::Content::VisMappings(inner) => Self::VisMappings(inner),
        }
    }
}

impl From<udp_packet::Content> for UpdatePacket {
    fn from(packet: udp_packet::Content) -> Self {
        match packet {
            udp_packet::Content::WorldState(inner) => Self::WorldState(inner),
            udp_packet::Content::VisUpdate(inner) => Self::VisualizationUpdate(inner),
        }
    }
}

pub async fn io_task(
    host: SocketAddr,
    packets_out: Sender<UpdatePacket>,
    requests_in: Receiver<ws_request::Content>,
) {
    // ======== Socket setup ========

    let mut udp_rx_buf = [0u8; 65535]; // Max size of an udp datagram

    // Start websocket connection
    let tcp_stream = async_net::TcpStream::connect(host)
        .await
        .unwrap_or_else(|_| panic!("Failed tcp connection to {host}"));
    let (websocket, _) = async_tungstenite::client_async(format!("ws://{host}"), tcp_stream)
        .await
        .unwrap_or_else(|_| panic!("Failed websocket connection to {host}"));
    let (mut ws_sender, ws_receiver) = websocket.split();

    // Bind udp socket to any free port
    let udp_socket = if host.is_ipv6() {
        UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 0)).await
    } else {
        UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await
    }
    .unwrap_or_else(|_| panic!("Failed to bind UDP socket for {host}"));
    let udp_port = udp_socket.local_addr().unwrap().port();

    // ======== Stream merging ========

    enum StreamEvent {
        WsRequest(ws_request::Content),
        WsPacket(ws_packet::Content),
        UdpPacket(udp_packet::Content),
        None,
    }

    #[derive(Debug)]
    #[allow(dead_code)] // The associated values are only for the Debug impl
    enum RxError {
        Tungstenite(tungstenite::Error),
        Io(io::Error),
        Decode(prost::DecodeError),
    }

    let ws_mapped = ws_receiver.map(|msg| {
        let message = msg.map_err(RxError::Tungstenite)?;

        match message {
            tungstenite::Message::Text(_) => {
                debug!("Received unexpected text message from {host}");
                Ok(StreamEvent::None)
            }
            tungstenite::Message::Binary(bytes) => {
                let packet = WsPacket::decode(bytes).map_err(RxError::Decode)?;
                if let Some(packet_content) = packet.content {
                    Ok(StreamEvent::WsPacket(packet_content))
                } else {
                    debug!("Received empty oneof protobuf field");
                    Ok(StreamEvent::None)
                }
            }
            tungstenite::Message::Ping(_) => {
                // The pong response is sent automatically
                Ok(StreamEvent::None)
            }
            tungstenite::Message::Pong(_) => {
                debug!("Received unexpected pong message from {host}. The server should be the one *initiating* pings.");
                Ok(StreamEvent::None)
            }
            tungstenite::Message::Close(_) => {
                // TODO: Handle close messages
                warn!("Proper websocket close handling is not implement yet");
                Ok(StreamEvent::None)
            }
            tungstenite::Message::Frame(_) => {
                unreachable!("Frame messages should never be passed through to user code")
            }
        }
    }).filter(|r| r.is_err() || r.as_ref().is_ok_and(|e| !matches!(e, StreamEvent::None)));

    // Hack to generate a packet stream from an udp socket. The socket is passed along as state.
    let udp_mapped = stream::unfold(&udp_socket, |sock| async move {
        let result = sock
            .recv_from(&mut udp_rx_buf)
            .await
            .map_err(RxError::Io)
            .and_then(|(size, _)| UdpPacket::decode(&udp_rx_buf[..size]).map_err(RxError::Decode))
            .map(|p| {
                if let Some(packet_content) = p.content {
                    StreamEvent::UdpPacket(packet_content)
                } else {
                    debug!("Received empty oneof protobuf field");
                    StreamEvent::None
                }
            });
        Some((result, sock))
    });

    let req_mapped = requests_in.map(|r| Ok(StreamEvent::WsRequest(r)));

    let mut combined_stream = ws_mapped.or(udp_mapped).or(req_mapped).boxed();

    // ======== Event processing ========

    let mut warn_cooldown = Instant::now();

    // Returns false if the receiver was dropped and the thread sould be stopped
    let mut packet_out_send = |packet: UpdatePacket| match packets_out.try_send(packet) {
        Ok(_) => true,
        Err(TrySendError::Full(_)) => {
            if warn_cooldown < Instant::now() {
                warn!("Status rx channel for {host} full (system can't keep up)");
                warn_cooldown = Instant::now() + Duration::from_secs(5);
            }
            true
        }
        Err(TrySendError::Closed(_)) => {
            debug!("Packet receiver for {host} dropped, stopping io task");
            false
        }
    };

    while let Some(event) = combined_stream
        .next()
        .or(async {
            // At least a ping should be received every second
            async_io::Timer::after(Duration::from_millis(1500)).await;
            None
        })
        .await
    {
        let event = match event {
            Ok(e) => e,
            Err(e) => {
                error!("Network error: {e:?}");
                return;
            }
        };
        match event {
            StreamEvent::WsRequest(mut request_content) => {
                // Outgoing: Send the request to the WebSocket server
                if let ws_request::Content::UdpStreamReq(req) = &request_content {
                    request_content = ws_request::Content::UdpStreamReq(UdpStreamRequest {
                        port: udp_port as u32,
                        ..req.clone()
                    });
                }
                let request = WsRequest {
                    content: Some(request_content),
                };
                let mut buf = BytesMut::new();
                if request.encode(&mut buf).is_ok() {
                    ws_sender
                        .send(tungstenite::Message::Binary(buf.into()))
                        .await
                        .expect("Websocket closed");
                }
            }
            StreamEvent::WsPacket(packet) => {
                if !packet_out_send(packet.into()) {
                    return;
                }
            }
            StreamEvent::UdpPacket(packet) => {
                if !packet_out_send(packet.into()) {
                    return;
                }
            }
            StreamEvent::None => {}
        }
    }

    info!("Connection to {host} timed out");
}
