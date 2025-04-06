use socket2::{Domain, Protocol, Socket, Type};
use std::net::{Ipv6Addr, ToSocketAddrs};
use std::{io, mem};

pub trait SSMSocketExtension<T> {
    /// Creates and binds a new socket with all socket options required for multicast to work as expected
    fn bind_multicast(addr: impl ToSocketAddrs) -> io::Result<T>;
    /// [join_multicast_v6](std::net::udp::UdpSocket::join_multicast_v6), but for [source-specific multicast](https://datatracker.ietf.org/doc/html/rfc4607) instead of the usual any-source multicast
    fn join_ssm_v6(&self, multiaddr: Ipv6Addr, source: Ipv6Addr, if_index: u32) -> io::Result<()>;
}

#[cfg(unix)]
use {
    libc::{
        AF_INET6, IPPROTO_IPV6, MCAST_JOIN_SOURCE_GROUP, c_int, in6_addr, setsockopt, sockaddr_in6,
        sockaddr_storage, socklen_t,
    },
    std::os::fd::{AsRawFd, OwnedFd},
};

#[cfg(unix)]
fn handle_sockerr(res: c_int) -> io::Result<()> {
    if res == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
impl<T: AsRawFd + TryFrom<OwnedFd, Error = io::Error>> SSMSocketExtension<T> for T {
    fn bind_multicast(addr: impl ToSocketAddrs) -> io::Result<T> {
        let mut last_err = None;
        for addr in addr.to_socket_addrs()? {
            let raw_socket = Socket::new(
                if addr.is_ipv6() {
                    Domain::IPV6
                } else {
                    Domain::IPV4
                },
                Type::DGRAM,
                Some(Protocol::UDP),
            )?;
            // Setting SO_REUSEADDR is required for multiple sockets to listen to the same multicast address/port
            // For multicast specially, this causes every received packet to be delivered to every joined socket.
            raw_socket.set_reuse_address(true)?;
            match raw_socket.bind(&addr.into()) {
                Ok(_) => return OwnedFd::from(raw_socket).try_into(),
                Err(err) => last_err = Some(err),
            }
        }

        Err(last_err.unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "could not bind to any of the addresses",
            )
        }))
    }

    fn join_ssm_v6(&self, group: Ipv6Addr, source: Ipv6Addr, if_index: u32) -> io::Result<()> {
        fn ipv6addr_to_sockaddr_storage(addr: Ipv6Addr, if_index: u32) -> sockaddr_storage {
            let mut storage: sockaddr_storage = unsafe { mem::zeroed() };

            unsafe {
                let c_addr = &mut storage as *mut sockaddr_storage as *mut sockaddr_in6;

                (*c_addr).sin6_family = AF_INET6 as u16;
                (*c_addr).sin6_addr = in6_addr {
                    s6_addr: addr.octets(),
                };
                (*c_addr).sin6_scope_id = if_index;
            }

            storage
        }

        // This struct is not yet included in the libc crate
        #[repr(C)]
        struct GroupSourceReq {
            gsr_interface: u32,
            gsr_group: sockaddr_storage,
            gsr_source: sockaddr_storage,
        }

        let req = GroupSourceReq {
            gsr_interface: if_index,
            gsr_group: ipv6addr_to_sockaddr_storage(group, if_index),
            gsr_source: ipv6addr_to_sockaddr_storage(source, if_index),
        };

        match unsafe {
            setsockopt(
                self.as_raw_fd(),
                IPPROTO_IPV6,
                MCAST_JOIN_SOURCE_GROUP,
                (&req as *const GroupSourceReq).cast(),
                size_of::<GroupSourceReq>() as socklen_t,
            )
        } {
            -1 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        }
    }
}

#[cfg(windows)]
use {
    std::os::windows::io::{AsRawSocket, OwnedSocket},
    windows_sys::Win32::Networking::WinSock::{
        AF_INET6, GROUP_SOURCE_REQ, IN6_ADDR, IN6_ADDR_0, IPPROTO_IPV6, MCAST_JOIN_SOURCE_GROUP,
        SOCKADDR_IN6, SOCKADDR_IN6_0, SOCKADDR_STORAGE, SOCKET, SOCKET_ERROR, setsockopt,
    },
};

#[cfg(windows)]
fn handle_sockerr(res: i32) -> io::Result<()> {
    if res == SOCKET_ERROR {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
impl<T: AsRawSocket + TryFrom<OwnedSocket, Error = io::Error>> SSMSocketExtension<T> for T {
    fn bind_multicast(addr: impl ToSocketAddrs) -> io::Result<T> {
        let mut last_err = None;
        for addr in addr.to_socket_addrs()? {
            let raw_socket = Socket::new(
                if addr.is_ipv6() {
                    Domain::IPV6
                } else {
                    Domain::IPV4
                },
                Type::DGRAM,
                Some(Protocol::UDP),
            )?;
            // Setting SO_REUSEADDR is required for multiple sockets to listen to the same multicast address/port.
            // For multicast specially, this causes every received packet to be delivered to every joined socket.
            // On windows, there is also SO_REUSE_MULTICASTPORT, but I just can't figure out how it should be used.
            raw_socket.set_reuse_address(true)?;
            match raw_socket.bind(&addr.into()) {
                Ok(_) => return OwnedSocket::from(raw_socket).try_into(),
                Err(err) => last_err = Some(err),
            }
        }

        Err(last_err.unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "could not bind to any of the addresses",
            )
        }))
    }

    fn join_ssm_v6(&self, multiaddr: Ipv6Addr, source: Ipv6Addr, if_index: u32) -> io::Result<()> {
        fn ipv6addr_to_sockaddr_storage(addr: Ipv6Addr, if_index: u32) -> SOCKADDR_STORAGE {
            let mut storage: SOCKADDR_STORAGE = unsafe { mem::zeroed() };

            unsafe {
                let c_addr = &mut storage as *mut SOCKADDR_STORAGE as *mut SOCKADDR_IN6;

                (*c_addr).sin6_family = AF_INET6;
                (*c_addr).sin6_addr = IN6_ADDR {
                    u: IN6_ADDR_0 {
                        Byte: addr.octets(),
                    },
                };
                (*c_addr).Anonymous = SOCKADDR_IN6_0 {
                    sin6_scope_id: if_index,
                };
            }

            storage
        }

        let req = GROUP_SOURCE_REQ {
            gsr_interface: if_index,
            gsr_group: ipv6addr_to_sockaddr_storage(multiaddr, if_index),
            gsr_source: ipv6addr_to_sockaddr_storage(source, if_index),
        };

        handle_sockerr(unsafe {
            setsockopt(
                self.as_raw_socket() as SOCKET,
                IPPROTO_IPV6,
                MCAST_JOIN_SOURCE_GROUP as i32,
                (&req as *const GROUP_SOURCE_REQ).cast(),
                size_of::<GROUP_SOURCE_REQ>() as i32,
            )
        })
    }
}
