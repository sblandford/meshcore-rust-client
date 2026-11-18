//! Bluetooth LE on Linux via BlueZ, spoken over our own minimal D-Bus client.

use super::dbus::{Bus, Msg, AGENT_PATH, SIGNAL, V};
use super::{Found, LinkEvent, Notify, NUS_RX, NUS_SERVICE, NUS_TX};
use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BLUEZ: &str = "org.bluez";
const PROPS: &str = "org.freedesktop.DBus.Properties";

fn managed_objects(bus: &mut Bus) -> Result<V, String> {
    let r = bus.call(Msg::call(BLUEZ, "/", "org.freedesktop.DBus.ObjectManager", "GetManagedObjects", vec![]), 10_000)?;
    r.body.into_iter().next().ok_or_else(|| "empty GetManagedObjects reply".into())
}

/// Iterate (object path, interfaces dict) pairs.
fn objects(all: &V) -> impl Iterator<Item = (&str, &V)> {
    all.items().iter().filter_map(|e| match e {
        V::E(k, v) => Some((k.str()?, &**v)),
        _ => None,
    })
}

fn adapter(all: &V) -> Result<String, String> {
    let (path, ifs) = objects(all)
        .find(|(_, ifs)| ifs.get("org.bluez.Adapter1").is_some())
        .ok_or("no Bluetooth adapter found")?;
    if ifs.get("org.bluez.Adapter1").and_then(|a| a.get("Powered")).and_then(V::bool) == Some(false) {
        return Err("the Bluetooth adapter is switched off".into());
    }
    Ok(path.to_string())
}

fn is_meshcore(dev: &V) -> bool {
    let has_uuid = dev.get("UUIDs").map(|u| u.items().iter().any(|x| x.str().map(str::to_lowercase).as_deref() == Some(NUS_SERVICE))).unwrap_or(false);
    let name = dev.get("Name").and_then(V::str).unwrap_or("");
    has_uuid || name.starts_with("MeshCore")
}

pub fn scan(results: Arc<Mutex<Vec<Found>>>, stop: &AtomicBool, wake: Arc<dyn Fn() + Send + Sync>) -> Result<(), String> {
    let mut bus = Bus::system()?;
    let all = managed_objects(&mut bus)?;
    let ad = adapter(&all)?;
    let _ = bus.call(Msg::call(BLUEZ, &ad, "org.bluez.Adapter1", "SetDiscoveryFilter", vec![V::dict(vec![("Transport", V::S("le".into()))])]), 5000);
    if let Err(e) = bus.call(Msg::call(BLUEZ, &ad, "org.bluez.Adapter1", "StartDiscovery", vec![]), 5000) {
        if !e.contains("InProgress") {
            return Err(format!("cannot start scanning: {e}"));
        }
    }
    let mut res = Ok(());
    while !super::stopped(stop) {
        match managed_objects(&mut bus) {
            Ok(all) => {
                let mut changed = false;
                for (path, ifs) in objects(&all) {
                    let Some(dev) = ifs.get("org.bluez.Device1") else { continue };
                    if !path.starts_with(&ad) || !is_meshcore(dev) {
                        continue;
                    }
                    let name = dev.get("Name").or(dev.get("Alias")).and_then(V::str).unwrap_or("?").to_string();
                    changed |= super::upsert(&results, Found {
                        id: path.to_string(),
                        name,
                        rssi: dev.get("RSSI").and_then(V::int).map(|r| r as i16),
                        paired: dev.get("Paired").and_then(V::bool).unwrap_or(false),
                    });
                }
                if changed {
                    wake();
                }
            }
            Err(e) => {
                res = Err(e);
                break;
            }
        }
        bus.queue.clear();
        std::thread::sleep(Duration::from_millis(700));
    }
    let _ = bus.call(Msg::call(BLUEZ, &ad, "org.bluez.Adapter1", "StopDiscovery", vec![]), 3000);
    res
}

fn dev_prop(bus: &mut Bus, dev: &str, prop: &str) -> Option<V> {
    let r = bus.call(Msg::call(BLUEZ, dev, PROPS, "Get", vec![V::S("org.bluez.Device1".into()), V::S(prop.into())]), 5000).ok()?;
    r.body.into_iter().next()
}

pub fn run(dev: &str, pin: u32, rx: Receiver<Vec<u8>>, stop: &AtomicBool, notify: &Notify) -> Result<(), String> {
    let status = |s: &str| notify(LinkEvent::Status(s.into()));
    let mut bus = Bus::system()?;
    bus.agent_pin = Some(pin);
    // Our own agent answers the PIN request if BlueZ needs to pair during this session.
    let _ = bus.call(Msg::call(BLUEZ, "/org/bluez", "org.bluez.AgentManager1", "RegisterAgent", vec![V::O(AGENT_PATH.into()), V::S("KeyboardDisplay".into())]), 5000);

    // If another application already holds the connection, leave it connected when we finish.
    let was_connected = dev_prop(&mut bus, dev, "Connected").and_then(|v| v.bool()) == Some(true);
    let result = session(&mut bus, dev, rx, stop, notify, &status);

    if !was_connected {
        let _ = bus.call(Msg::call(BLUEZ, dev, "org.bluez.Device1", "Disconnect", vec![]), 5000);
    }
    let _ = bus.call(Msg::call(BLUEZ, "/org/bluez", "org.bluez.AgentManager1", "UnregisterAgent", vec![V::O(AGENT_PATH.into())]), 3000);
    result
}

fn session(bus: &mut Bus, dev: &str, rx: Receiver<Vec<u8>>, stop: &AtomicBool, notify: &Notify, status: &dyn Fn(&str)) -> Result<(), String> {
    let device = |m: &str| Msg::call(BLUEZ, dev, "org.bluez.Device1", m, vec![]);
    if dev_prop(bus, dev, "Connected").and_then(|v| v.bool()) != Some(true) {
        status("Connecting…");
        let mut tries = 0;
        loop {
            match bus.call(device("Connect"), 30_000) {
                Ok(_) => break,
                Err(e) if tries < 2 && !super::stopped(stop) => {
                    tries += 1;
                    status(&format!("Connect failed ({e}), retrying…"));
                    std::thread::sleep(Duration::from_millis(800));
                }
                Err(e) => return Err(format!("connect failed: {e}")),
            }
        }
    }
    if dev_prop(bus, dev, "Paired").and_then(|v| v.bool()) != Some(true) {
        status("Pairing…");
        match bus.call(device("Pair"), 60_000) {
            Ok(_) => {
                let _ = bus.call(Msg::call(BLUEZ, dev, PROPS, "Set", vec![V::S("org.bluez.Device1".into()), V::S("Trusted".into()), V::Var(Box::new(V::B(true)))]), 5000);
            }
            Err(e) if e.contains("AlreadyExists") => {}
            Err(e) if e.contains("Authentication") => return Err(format!("pairing failed – check the PIN ({e})")),
            Err(e) => status(&format!("Pairing skipped: {e}")),
        }
    }
    status("Discovering services…");
    let t0 = Instant::now();
    while dev_prop(bus, dev, "ServicesResolved").and_then(|v| v.bool()) != Some(true) {
        if t0.elapsed() > Duration::from_secs(20) || super::stopped(stop) {
            return Err("timed out waiting for GATT services".into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let all = managed_objects(bus)?;
    let (mut rx_char, mut tx_char) = (None, None);
    for (path, ifs) in objects(&all) {
        if !path.starts_with(dev) {
            continue;
        }
        if let Some(uuid) = ifs.get("org.bluez.GattCharacteristic1").and_then(|c| c.get("UUID")).and_then(V::str) {
            match uuid.to_lowercase().as_str() {
                NUS_RX => rx_char = Some(path.to_string()),
                NUS_TX => tx_char = Some(path.to_string()),
                _ => {}
            }
        }
    }
    let (Some(rx_char), Some(tx_char)) = (rx_char, tx_char) else {
        return Err("device has no MeshCore (Nordic UART) service".into());
    };
    bus.add_match(&format!("type='signal',interface='{PROPS}',member='PropertiesChanged',path='{tx_char}'"))?;
    bus.add_match(&format!("type='signal',interface='{PROPS}',member='PropertiesChanged',path='{dev}'"))?;
    let shared = bus
        .call(Msg::call(BLUEZ, &tx_char, PROPS, "Get", vec![V::S("org.bluez.GattCharacteristic1".into()), V::S("Notifying".into())]), 5000)
        .ok()
        .and_then(|r| r.body.first().and_then(V::bool))
        == Some(true);
    if shared {
        status("Warning: another application on this computer is already using this radio; close it to avoid mixed-up replies");
    }
    bus.call(Msg::call(BLUEZ, &tx_char, "org.bluez.GattCharacteristic1", "StartNotify", vec![]), 10_000)
        .map_err(|e| format!("cannot subscribe to notifications: {e}"))?;
    notify(LinkEvent::Connected);

    let mut outq: VecDeque<Vec<u8>> = VecDeque::new();
    let mut inflight: Option<(u32, Instant)> = None;
    // BlueZ emits one PropertiesChanged per notify subscriber, so when another app is also
    // subscribed every notification arrives more than once. Identical back-to-back values
    // with no write in between are duplicates (we only ever have one command outstanding).
    let mut last_rx: Option<(Vec<u8>, Instant)> = None;
    let result = loop {
        if super::stopped(stop) {
            break Ok(());
        }
        outq.extend(super::drain(&rx));
        if inflight.is_none() {
            if let Some(f) = outq.pop_front() {
                let m = Msg::call(BLUEZ, &rx_char, "org.bluez.GattCharacteristic1", "WriteValue", vec![
                    V::A("y".into(), f.into_iter().map(V::Y).collect()),
                    V::dict(vec![("type", V::S("request".into()))]),
                ]);
                inflight = Some((bus.send(&m)?, Instant::now()));
                last_rx = None;
            }
        }
        if let Some((_, t)) = inflight {
            if t.elapsed() > Duration::from_secs(10) {
                break Err("write timed out".into());
            }
        }
        let Some(m) = bus.next(if outq.is_empty() { 40 } else { 5 })? else { continue };
        if m.kind == SIGNAL && m.member == "PropertiesChanged" {
            let changed = m.body.get(1);
            if m.path == tx_char {
                if let Some(v) = changed.and_then(|c| c.get("Value")) {
                    let frame = v.bytes();
                    let dup = matches!(&last_rx, Some((p, t)) if *p == frame && t.elapsed() < Duration::from_millis(250));
                    last_rx = Some((frame.clone(), Instant::now()));
                    if !dup {
                        notify(LinkEvent::Frame(frame));
                    }
                }
            } else if m.path == dev && changed.and_then(|c| c.get("Connected")).and_then(V::bool) == Some(false) {
                break Err("device disconnected".into());
            }
        } else if inflight.map(|(s, _)| Some(s) == m.reply_serial).unwrap_or(false) {
            inflight = None;
            if !m.error.is_empty() {
                let detail = m.body.first().and_then(V::str).unwrap_or("");
                status(&format!("Write failed: {} {detail}", m.error));
            }
        }
    };
    let _ = bus.call(Msg::call(BLUEZ, &tx_char, "org.bluez.GattCharacteristic1", "StopNotify", vec![]), 3000);
    result
}
