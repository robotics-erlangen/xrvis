use std::io;

pub mod interface_flags;
pub mod ssm_socket;

#[cfg(unix)]
fn map_sockerr(res: libc::c_int) -> io::Result<libc::c_int> {
    if res == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(res)
    }
}

#[cfg(windows)]
fn map_sockerr(res: i32) -> io::Result<i32> {
    if res == windows_sys::Win32::Networking::WinSock::SOCKET_ERROR {
        Err(io::Error::last_os_error())
    } else {
        Ok(res)
    }
}
