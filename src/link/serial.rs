//! Minimal USB-serial port support (115200 8N1, raw), written directly against
//! termios on Unix and the Win32 comm API on Windows.

use super::{LinkEvent, Notify};
use crate::proto::{serial_wrap, SerialDeframer};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;

/// (device path, human description)
pub fn list_ports() -> Vec<(String, String)> {
    let mut v = sys::list_ports();
    v.sort();
    v
}

pub fn run(path: &str, rx: Receiver<Vec<u8>>, stop: &AtomicBool, notify: &Notify) -> Result<(), String> {
    notify(LinkEvent::Status(format!("Opening {path}…")));
    let mut port = sys::Port::open(path).map_err(|e| format!("{path}: {e}"))?;
    notify(LinkEvent::Connected);
    let mut de = SerialDeframer::default();
    let mut buf = [0u8; 1024];
    while !super::stopped(stop) {
        for f in super::drain(&rx) {
            port.write_all(&serial_wrap(&f)).map_err(|e| format!("write: {e}"))?;
        }
        let n = port.read(&mut buf, 30).map_err(|e| format!("read: {e}"))?;
        if n > 0 {
            de.push(&buf[..n], |f| notify(LinkEvent::Frame(f)));
        }
    }
    Ok(())
}

#[cfg(unix)]
mod sys {
    use std::ffi::CString;
    use std::io::{Error, ErrorKind, Result};

    pub struct Port {
        fd: libc::c_int,
    }

    impl Port {
        pub fn open(path: &str) -> Result<Port> {
            let c = CString::new(path).map_err(|_| Error::from(ErrorKind::InvalidInput))?;
            unsafe {
                let fd = libc::open(c.as_ptr(), libc::O_RDWR | libc::O_NOCTTY | libc::O_NONBLOCK | libc::O_CLOEXEC);
                if fd < 0 {
                    return Err(Error::last_os_error());
                }
                let port = Port { fd };
                libc::ioctl(fd, libc::TIOCEXCL as _);
                let mut t: libc::termios = std::mem::zeroed();
                if libc::tcgetattr(fd, &mut t) != 0 {
                    return Err(Error::last_os_error());
                }
                libc::cfmakeraw(&mut t);
                t.c_cflag |= libc::CLOCAL | libc::CREAD;
                t.c_cflag &= !(libc::CRTSCTS | libc::CSTOPB | libc::PARENB);
                t.c_iflag &= !(libc::IXON | libc::IXOFF | libc::IXANY);
                t.c_cc[libc::VMIN] = 0;
                t.c_cc[libc::VTIME] = 0;
                libc::cfsetispeed(&mut t, libc::B115200);
                libc::cfsetospeed(&mut t, libc::B115200);
                if libc::tcsetattr(fd, libc::TCSANOW, &t) != 0 {
                    return Err(Error::last_os_error());
                }
                libc::tcflush(fd, libc::TCIOFLUSH);
                Ok(port)
            }
        }

        fn poll(&self, events: libc::c_short, ms: i32) -> Result<libc::c_short> {
            let mut p = libc::pollfd { fd: self.fd, events, revents: 0 };
            let r = unsafe { libc::poll(&mut p, 1, ms) };
            if r < 0 {
                let e = Error::last_os_error();
                return if e.kind() == ErrorKind::Interrupted { Ok(0) } else { Err(e) };
            }
            if p.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
                return Err(Error::new(ErrorKind::BrokenPipe, "device disconnected"));
            }
            Ok(p.revents)
        }

        /// Read whatever is available, waiting up to `ms` milliseconds.
        pub fn read(&mut self, buf: &mut [u8], ms: i32) -> Result<usize> {
            if self.poll(libc::POLLIN, ms)? & libc::POLLIN == 0 {
                return Ok(0);
            }
            let n = unsafe { libc::read(self.fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n < 0 {
                let e = Error::last_os_error();
                return if e.kind() == ErrorKind::WouldBlock { Ok(0) } else { Err(e) };
            }
            if n == 0 {
                return Err(Error::new(ErrorKind::BrokenPipe, "device disconnected"));
            }
            Ok(n as usize)
        }

        pub fn write_all(&mut self, mut data: &[u8]) -> Result<()> {
            while !data.is_empty() {
                let n = unsafe { libc::write(self.fd, data.as_ptr().cast(), data.len()) };
                if n < 0 {
                    let e = Error::last_os_error();
                    if e.kind() == ErrorKind::WouldBlock {
                        self.poll(libc::POLLOUT, 500)?;
                        continue;
                    }
                    return Err(e);
                }
                data = &data[n as usize..];
            }
            Ok(())
        }
    }

    impl Drop for Port {
        fn drop(&mut self) {
            unsafe {
                libc::close(self.fd);
            }
        }
    }

    #[cfg(target_os = "linux")]
    pub fn list_ports() -> Vec<(String, String)> {
        let mut v = Vec::new();
        let Ok(rd) = std::fs::read_dir("/sys/class/tty") else { return v };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !(name.starts_with("ttyACM") || name.starts_with("ttyUSB")) {
                continue;
            }
            let dev = e.path().join("device");
            let read = |p: std::path::PathBuf| std::fs::read_to_string(p).ok().map(|s| s.trim().to_string());
            let desc = [dev.join("../product"), dev.join("../../product")]
                .into_iter()
                .find_map(read)
                .unwrap_or_default();
            let maker = [dev.join("../manufacturer"), dev.join("../../manufacturer")]
                .into_iter()
                .find_map(read)
                .unwrap_or_default();
            v.push((format!("/dev/{name}"), format!("{maker} {desc}").trim().to_string()));
        }
        v
    }

    #[cfg(not(target_os = "linux"))]
    pub fn list_ports() -> Vec<(String, String)> {
        let mut v = Vec::new();
        let Ok(rd) = std::fs::read_dir("/dev") else { return v };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("cu.") && !name.contains("Bluetooth") && !name.contains("debug-console") {
                v.push((format!("/dev/{name}"), String::new()));
            }
        }
        v
    }
}

#[cfg(windows)]
mod sys {
    use std::io::{Error, ErrorKind, Result};
    use windows::core::{HSTRING, PCWSTR, PWSTR};
    use windows::Win32::Devices::Communication::*;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE};
    use windows::Win32::Storage::FileSystem::*;
    use windows::Win32::System::Registry::*;

    pub struct Port {
        h: HANDLE,
        timeout_ms: u32,
    }

    fn werr(e: windows::core::Error) -> Error {
        Error::other(e.message())
    }

    impl Port {
        pub fn open(path: &str) -> Result<Port> {
            let name = HSTRING::from(format!("\\\\.\\{path}"));
            let h = unsafe {
                CreateFileW(&name, (GENERIC_READ | GENERIC_WRITE).0, FILE_SHARE_MODE(0), None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None)
            }
            .map_err(werr)?;
            let mut port = Port { h, timeout_ms: 0 };
            unsafe {
                let mut dcb = DCB { DCBlength: std::mem::size_of::<DCB>() as u32, ..Default::default() };
                GetCommState(h, &mut dcb).map_err(werr)?;
                dcb.BaudRate = 115_200;
                dcb.ByteSize = 8;
                dcb.Parity = NOPARITY;
                dcb.StopBits = ONESTOPBIT;
                // fBinary | fDtrControl=ENABLE | fRtsControl=ENABLE
                dcb._bitfield = 1 | (1 << 4) | (1 << 12);
                SetCommState(h, &dcb).map_err(werr)?;
                let _ = PurgeComm(h, PURGE_RXCLEAR | PURGE_TXCLEAR);
            }
            port.set_timeout(30)?;
            Ok(port)
        }

        fn set_timeout(&mut self, ms: u32) -> Result<()> {
            if self.timeout_ms == ms {
                return Ok(());
            }
            // Return immediately if bytes are waiting, otherwise wait up to `ms`.
            let t = COMMTIMEOUTS {
                ReadIntervalTimeout: u32::MAX,
                ReadTotalTimeoutMultiplier: u32::MAX,
                ReadTotalTimeoutConstant: ms.max(1),
                WriteTotalTimeoutMultiplier: 0,
                WriteTotalTimeoutConstant: 2000,
            };
            unsafe { SetCommTimeouts(self.h, &t) }.map_err(werr)?;
            self.timeout_ms = ms;
            Ok(())
        }

        pub fn read(&mut self, buf: &mut [u8], ms: i32) -> Result<usize> {
            self.set_timeout(ms as u32)?;
            let mut n = 0u32;
            unsafe { ReadFile(self.h, Some(buf), Some(&mut n), None) }.map_err(werr)?;
            Ok(n as usize)
        }

        pub fn write_all(&mut self, mut data: &[u8]) -> Result<()> {
            while !data.is_empty() {
                let mut n = 0u32;
                unsafe { WriteFile(self.h, Some(data), Some(&mut n), None) }.map_err(werr)?;
                if n == 0 {
                    return Err(Error::new(ErrorKind::TimedOut, "write timed out"));
                }
                data = &data[n as usize..];
            }
            Ok(())
        }
    }

    impl Drop for Port {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.h);
            }
        }
    }

    pub fn list_ports() -> Vec<(String, String)> {
        let mut v = Vec::new();
        unsafe {
            let mut key = HKEY::default();
            let sub = HSTRING::from("HARDWARE\\DEVICEMAP\\SERIALCOMM");
            if RegOpenKeyExW(HKEY_LOCAL_MACHINE, PCWSTR(sub.as_ptr()), None, KEY_READ, &mut key).is_err() {
                return v;
            }
            for i in 0.. {
                let mut name = [0u16; 256];
                let mut name_len = name.len() as u32;
                let mut data = [0u8; 256];
                let mut data_len = data.len() as u32;
                let r = RegEnumValueW(
                    key,
                    i,
                    Some(PWSTR(name.as_mut_ptr())),
                    &mut name_len,
                    None,
                    None,
                    Some(data.as_mut_ptr()),
                    Some(&mut data_len),
                );
                if r.is_err() {
                    break;
                }
                let wide: Vec<u16> = data[..data_len as usize].chunks(2).map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
                let port = String::from_utf16_lossy(&wide).trim_end_matches('\0').to_string();
                let dev = String::from_utf16_lossy(&name[..name_len as usize]);
                v.push((port, dev.rsplit('\\').next().unwrap_or("").to_string()));
            }
            let _ = RegCloseKey(key);
        }
        v
    }
}
