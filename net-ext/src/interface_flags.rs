use network_interface::NetworkInterface;
use std::{io, ptr};

pub trait NetworkInterfaceFlagExtension {
    fn is_multicast(&self) -> bool;
    fn is_up(&self) -> bool;
}

#[cfg(unix)]
use {
    super::map_sockerr,
    libc::{c_char, c_short, close, ifreq, ioctl, socket},
};

#[cfg(unix)]
fn get_if_flags(if_name: &str) -> io::Result<c_short> {
    let c_if_name = std::ffi::CString::new(if_name)?;

    let mut ifreq: ifreq = unsafe { std::mem::zeroed() };
    let if_name_len = c_if_name.as_bytes().len().min(libc::IFNAMSIZ - 1); // -1 to reserve space for the null terminator
    unsafe {
        ptr::copy_nonoverlapping(
            c_if_name.as_ptr(),
            ifreq.ifr_name.as_mut_ptr() as *mut c_char,
            if_name_len,
        );
    }

    unsafe {
        let socket = map_sockerr(socket(libc::AF_INET6, libc::SOCK_DGRAM, 0))?;
        // For some reason, the android libc SIOCGIFFLAGS does not match the type expected by ioctl.
        #[cfg(target_os = "android")]
        let res = map_sockerr(ioctl(socket, libc::SIOCGIFFLAGS as libc::c_int, &ifreq));
        #[cfg(not(target_os = "android"))]
        let res = map_sockerr(ioctl(socket, libc::SIOCGIFFLAGS, &ifreq));
        close(socket);
        res?;
    }

    Ok(unsafe { ifreq.ifr_ifru.ifru_flags })
}

#[cfg(unix)]
impl NetworkInterfaceFlagExtension for NetworkInterface {
    fn is_multicast(&self) -> bool {
        get_if_flags(&self.name).is_ok_and(|flags| flags & libc::IFF_MULTICAST as c_short != 0)
    }

    fn is_up(&self) -> bool {
        get_if_flags(&self.name).is_ok_and(|flags| flags & libc::IFF_UP as c_short != 0)
    }
}

#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS},
    NetworkManagement::{
        IpHelper::{GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH, IP_ADAPTER_NO_MULTICAST},
        Ndis::IfOperStatusUp,
    },
    Networking::WinSock::AF_UNSPEC,
};

#[cfg(windows)]
fn access_adapter_by_index<T>(
    if_index: u32,
    value_map: fn(*const IP_ADAPTER_ADDRESSES_LH) -> T,
) -> io::Result<T> {
    // 15kb is the recommended initial buffer size: https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-getadaptersaddresses#remarks
    let mut buf = vec![0u8; 15000];
    let mut buf_size: u32 = buf.len() as u32;
    let adapter_addresses = buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;

    // Make the call to fill the buffer. Retry with a resized buffer if failed.
    loop {
        let ret = unsafe {
            GetAdaptersAddresses(
                AF_UNSPEC as u32,
                0,
                ptr::null_mut(),
                adapter_addresses,
                &mut buf_size,
            )
        };
        match ret {
            ERROR_SUCCESS => break,
            ERROR_BUFFER_OVERFLOW => {
                buf.resize(buf_size as usize, 0);
                continue;
            }
            _ => return Err(io::Error::from_raw_os_error(ret as i32)),
        }
    }

    // Find the requested adapter from the linked list
    let mut curr_adapter = adapter_addresses;
    while !curr_adapter.is_null() {
        unsafe {
            if (*curr_adapter).Ipv6IfIndex == if_index
                || (*curr_adapter).Anonymous1.Anonymous.IfIndex == if_index
            {
                return Ok(value_map(curr_adapter));
            }
            curr_adapter = (*curr_adapter).Next;
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("network adapter with index {if_index} not found"),
    ))
}

#[cfg(windows)]
impl NetworkInterfaceFlagExtension for NetworkInterface {
    fn is_multicast(&self) -> bool {
        access_adapter_by_index(self.index, |a| unsafe { (*a).Anonymous2.Flags })
            .is_ok_and(|flags| flags & IP_ADAPTER_NO_MULTICAST == 0)
    }

    fn is_up(&self) -> bool {
        access_adapter_by_index(self.index, |a| unsafe { (*a).OperStatus })
            .is_ok_and(|oper_status| oper_status == IfOperStatusUp)
    }
}
