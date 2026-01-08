pub mod interface_flags;
pub mod ssm_socket;

#[cfg(unix)]
fn map_sockerr(res: libc::c_int) -> std::io::Result<libc::c_int> {
    if res == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(res)
    }
}

#[cfg(windows)]
fn map_sockerr(res: i32) -> std::io::Result<i32> {
    if res == windows_sys::Win32::Networking::WinSock::SOCKET_ERROR {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(res)
    }
}
