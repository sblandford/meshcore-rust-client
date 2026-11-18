//! Transports to the companion radio. Each open link runs on its own thread and
//! exchanges whole protocol frames with the UI through channels.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

pub mod serial;

#[cfg(target_os = "linux")]
mod dbus;
#[cfg(target_os = "linux")]
#[path = "ble_linux.rs"]
pub mod ble;
#[cfg(windows)]
#[path = "ble_windows.rs"]
pub mod ble;
#[cfg(target_os = "macos")]
#[path = "ble_macos.rs"]
pub mod ble;

/// BLE Nordic UART service used by MeshCore companion firmware.
#[cfg_attr(windows, allow(dead_code))]
pub const NUS_SERVICE: &str = "6e400001-b5a3-f393-e0a9-e50e24dcca9e";
/// App -> radio (write).
#[cfg_attr(windows, allow(dead_code))]
pub const NUS_RX: &str = "6e400002-b5a3-f393-e0a9-e50e24dcca9e";
/// Radio -> app (notify).
#[cfg_attr(windows, allow(dead_code))]
pub const NUS_TX: &str = "6e400003-b5a3-f393-e0a9-e50e24dcca9e";

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Serial(String),
    Ble { id: String, name: String, pin: u32 },
}

impl Target {
    pub fn label(&self) -> String {
        match self {
            Target::Serial(p) => format!("USB {p}"),
            Target::Ble { name, .. } => format!("BLE {name}"),
        }
    }
}

pub enum LinkEvent {
    Status(String),
    Connected,
    Frame(Vec<u8>),
    Closed(Option<String>),
}

/// Callback used by link threads to deliver events (also wakes the UI).
pub type Notify = Arc<dyn Fn(LinkEvent) + Send + Sync>;

/// Handle to an open link. Dropping it closes the connection.
pub struct Link {
    tx: Sender<Vec<u8>>,
    stop: Arc<AtomicBool>,
}

impl Link {
    pub fn open(target: Target, notify: Notify) -> Link {
        let (tx, rx) = channel::<Vec<u8>>();
        let stop = Arc::new(AtomicBool::new(false));
        let st = stop.clone();
        std::thread::Builder::new()
            .name("link".into())
            .spawn(move || {
                let res = match &target {
                    Target::Serial(path) => serial::run(path, rx, &st, &notify),
                    Target::Ble { id, pin, .. } => ble::run(id, *pin, rx, &st, &notify),
                };
                notify(LinkEvent::Closed(res.err()));
            })
            .expect("spawn link thread");
        Link { tx, stop }
    }

    pub fn send(&self, frame: Vec<u8>) {
        let _ = self.tx.send(frame);
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// A BLE device found while scanning.
#[derive(Clone, Debug)]
pub struct Found {
    pub id: String,
    pub name: String,
    pub rssi: Option<i16>,
    pub paired: bool,
}

/// Background BLE scan. Results accumulate in `results` until dropped.
pub struct Scanner {
    pub results: Arc<Mutex<Vec<Found>>>,
    pub error: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
}

impl Scanner {
    pub fn start(wake: Arc<dyn Fn() + Send + Sync>) -> Scanner {
        let results = Arc::new(Mutex::new(Vec::new()));
        let error = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (r, e, s) = (results.clone(), error.clone(), stop.clone());
        std::thread::Builder::new()
            .name("ble-scan".into())
            .spawn(move || {
                if let Err(err) = ble::scan(r, &s, wake.clone()) {
                    *e.lock().unwrap() = Some(err);
                    wake();
                }
            })
            .expect("spawn scan thread");
        Scanner { results, error, stop }
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Insert or refresh a scan result; returns true if the list changed.
pub(crate) fn upsert(list: &Mutex<Vec<Found>>, f: Found) -> bool {
    let mut l = list.lock().unwrap();
    if let Some(e) = l.iter_mut().find(|e| e.id == f.id) {
        let changed = e.name != f.name || e.rssi != f.rssi || e.paired != f.paired;
        if !f.name.is_empty() {
            e.name = f.name;
        }
        e.rssi = f.rssi.or(e.rssi);
        e.paired = f.paired;
        changed
    } else {
        l.push(f);
        true
    }
}

pub(crate) fn stopped(s: &AtomicBool) -> bool {
    s.load(Ordering::Relaxed)
}

pub(crate) fn drain(rx: &Receiver<Vec<u8>>) -> Vec<Vec<u8>> {
    rx.try_iter().collect()
}
