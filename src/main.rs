//! MeshCore desktop client: a small native app for MeshCore companion radios
//! over USB serial or Bluetooth LE.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod link;
mod proto;
mod store;
mod ui;
mod util;

/// `--scan`: list serial ports and MeshCore BLE radios on the console (diagnostics).
fn scan_cli() {
    println!("Serial ports:");
    for (p, d) in link::serial::list_ports() {
        println!("  {p}  {d}");
    }
    println!("Scanning for MeshCore BLE radios for 10 s…");
    let s = link::Scanner::start(std::sync::Arc::new(|| {}));
    std::thread::sleep(std::time::Duration::from_secs(10));
    if let Some(e) = s.error.lock().unwrap().clone() {
        println!("  error: {e}");
    }
    for f in s.results.lock().unwrap().iter() {
        println!("  {}  {}  rssi={:?} paired={}", f.name, f.id, f.rssi, f.paired);
    }
    drop(s);
    std::thread::sleep(std::time::Duration::from_millis(800));
}

/// `--probe <serial-port | BLE id>`: connect and run read-only queries (no message sync,
/// no settings changes), printing what the radio reports. For diagnostics.
fn probe_cli(arg: &str, pin: u32) {
    use link::{Link, LinkEvent, Target};
    use std::time::{Duration, Instant};
    let target = if arg.starts_with("/org/bluez") || arg.len() == 17 && arg.contains(':') || arg.starts_with('{') {
        Target::Ble { id: arg.into(), name: arg.into(), pin }
    } else {
        Target::Serial(arg.into())
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let link = Link::open(target, std::sync::Arc::new(move |e| {
        let _ = tx.send(e);
    }));
    let next = |want: &dyn Fn(&proto::Frame) -> bool| -> Option<proto::Frame> {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(40) {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(LinkEvent::Status(s)) => println!("[link] {s}"),
                Ok(LinkEvent::Connected) => println!("[link] connected"),
                Ok(LinkEvent::Closed(e)) => {
                    println!("[link] closed: {e:?}");
                    return None;
                }
                Ok(LinkEvent::Frame(f)) => match proto::parse(&f) {
                    Some(fr) if want(&fr) => return Some(fr),
                    Some(fr) => println!("  (other) {fr:?}"),
                    None => println!("  (unparsed) {}", util::hex(&f)),
                },
                Err(_) => {}
            }
        }
        println!("timeout");
        None
    };
    // Wait for the link to come up.
    let t = Instant::now();
    loop {
        match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(LinkEvent::Connected) => break,
            Ok(LinkEvent::Status(s)) => println!("[link] {s} ({:.1}s)", t.elapsed().as_secs_f32()),
            Ok(LinkEvent::Closed(e)) => return println!("[link] closed: {e:?}"),
            _ => return println!("timed out connecting"),
        }
    }
    println!("connected in {:.1}s", t.elapsed().as_secs_f32());
    link.send(proto::device_query());
    if let Some(proto::Frame::DeviceInfo(d)) = next(&|f| matches!(f, proto::Frame::DeviceInfo(_))) {
        println!("DEVICE: {d:?}");
        link.send(proto::app_start());
        if let Some(proto::Frame::SelfInfo(s)) = next(&|f| matches!(f, proto::Frame::SelfInfo(_))) {
            println!("SELF: name={} key={} freq={} kHz bw={} Hz sf={} cr={} tx={} dBm", s.name, util::hex(&s.pubkey[..8]), s.freq_khz, s.bw_hz, s.sf, s.cr, s.tx_power);
        }
        link.send(proto::simple(proto::CMD_GET_BATT_AND_STORAGE));
        println!("BATT: {:?}", next(&|f| matches!(f, proto::Frame::Batt { .. })));
        for i in 0..d.max_channels {
            link.send(proto::get_channel(i));
            if let Some(proto::Frame::ChannelInfo { idx, name, .. }) = next(&|f| matches!(f, proto::Frame::ChannelInfo { .. } | proto::Frame::Err(_))) {
                if !name.is_empty() {
                    println!("CHANNEL {idx}: {name}");
                }
            }
        }
        link.send(proto::get_contacts(0));
        let mut n = 0;
        while let Some(f) = next(&|f| matches!(f, proto::Frame::Contact(_) | proto::Frame::EndOfContacts)) {
            match f {
                proto::Frame::Contact(c) => {
                    n += 1;
                    if n <= 8 {
                        println!("CONTACT: {:<24} {:<12} {}", c.name, proto::type_name(c.kind), c.route_text());
                    }
                }
                _ => break,
            }
        }
        println!("{n} contacts");
    }
    drop(link);
    std::thread::sleep(Duration::from_secs(2));
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--scan") {
        scan_cli();
        return Ok(());
    }
    if let Some(i) = args.iter().position(|a| a == "--probe") {
        let pin = args.get(i + 2).and_then(|p| p.parse().ok()).unwrap_or(123456);
        probe_cli(args.get(i + 1).map(String::as_str).unwrap_or(""), pin);
        return Ok(());
    }
    let mut opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("MeshCore")
            // Must match the .desktop file / Flatpak ID so desktops show the right icon.
            .with_app_id("io.github.sblandford.MeshCoreClient")
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../packaging/icons/meshcore-256.png")).unwrap_or_default())
            .with_inner_size([1000.0, 680.0])
            .with_min_inner_size([640.0, 420.0]),
        ..Default::default()
    };
    // egui repaints only on demand, so vsync buys nothing here; it also makes the
    // GL swap block forever on some Wayland compositors (seen on KDE Plasma).
    opts.glow_options.vsync = false;
    eframe::run_native("MeshCore", opts, Box::new(|cc| Ok(Box::new(ui::Gui::new(cc)))))
}
