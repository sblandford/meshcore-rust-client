//! Rust Desktop Client for MeshCore: a small, independent native app for MeshCore
//! companion radios over USB serial or Bluetooth LE.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod fonts;
mod link;
mod proto;
mod store;
mod ui;
mod util;

/// User-visible application name. "MeshCore" is used only to say what the app works with.
pub const APP_NAME: &str = "Rust Desktop Client for MeshCore";
/// Shown wherever the app is presented, so it is never mistaken for the official app.
pub const DISCLAIMER: &str = "Independent open-source software, compatible with MeshCore radios. \
Not affiliated with or endorsed by the MeshCore project.";
pub const SOURCE_URL: &str = "https://github.com/sblandford/meshcore-rust-client";

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
        for (label, code) in [
            ("AUTO-ADD", proto::CMD_GET_AUTOADD_CONFIG),
            ("TUNING", proto::CMD_GET_TUNING_PARAMS),
            ("REGION", proto::CMD_GET_DEFAULT_FLOOD_SCOPE),
            ("SENSORS", proto::CMD_GET_CUSTOM_VARS),
            ("REPEAT FREQS", proto::CMD_GET_ALLOWED_REPEAT_FREQ),
            ("CLOCK", proto::CMD_GET_DEVICE_TIME),
        ] {
            link.send(proto::simple(code));
            let r = next(&|f| !matches!(f, proto::Frame::LogRx { .. } | proto::Frame::MsgWaiting | proto::Frame::Advert(_) | proto::Frame::NewAdvert(_)));
            println!("{label}: {r:?}");
        }
        link.send(proto::simple(proto::CMD_GET_BATT_AND_STORAGE));
        println!("BATT: {:?}", next(&|f| matches!(f, proto::Frame::Batt { .. })));
        let mut channels = Vec::new();
        for i in 0..d.max_channels {
            link.send(proto::get_channel(i));
            if let Some(proto::Frame::ChannelInfo { idx, name, secret }) = next(&|f| matches!(f, proto::Frame::ChannelInfo { .. } | proto::Frame::Err(_))) {
                if !name.is_empty() {
                    println!("CHANNEL {idx}: {name}");
                    channels.push((name, secret));
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
        // Listen to overheard traffic and check channel packets decrypt (counts only, no content).
        let secs = std::env::var("MC_PROBE_LISTEN").ok().and_then(|v| v.parse().ok()).unwrap_or(0u64);
        if secs > 0 {
            println!("Listening for {secs} s…");
            let (mut total, mut grp, mut ok) = (0, 0, 0);
            let mut per: std::collections::BTreeMap<String, u32> = Default::default();
            let t = Instant::now();
            while t.elapsed() < Duration::from_secs(secs) {
                if let Ok(LinkEvent::Frame(f)) = rx.recv_timeout(Duration::from_millis(500)) {
                    if let Some(proto::Frame::LogRx { raw, .. }) = proto::parse(&f) {
                        total += 1;
                        if let Some(p) = proto::parse_raw(&raw).filter(|p| p.payload_type == proto::PAYLOAD_GRP_TXT) {
                            grp += 1;
                            if let Some((name, _)) = channels.iter().find(|(_, s)| proto::decrypt_group_text(p.payload, s).is_some()) {
                                ok += 1;
                                *per.entry(name.clone()).or_default() += 1;
                            }
                        }
                    }
                }
            }
            println!("Heard {total} packets: {grp} channel posts, {ok} decrypted with your channel keys {per:?}");
        }
    }
    drop(link);
    std::thread::sleep(Duration::from_secs(2));
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    // `--check-fonts <text>`: report which system fonts would be loaded to draw <text>.
    if let Some(i) = args.iter().position(|a| a == "--check-fonts") {
        let ctx = eframe::egui::Context::default();
        let mut f = fonts::Fonts::install(&ctx);
        let text = args.get(i + 1).cloned().unwrap_or_default();
        f.ensure(&ctx, [text.as_str()]);
        for name in f.loaded_names() {
            println!("loaded {name}");
        }
        for c in text.chars().filter(|c| !c.is_whitespace() && !f.can_draw(*c)) {
            println!("cannot draw {c:?} (U+{:04X})", c as u32);
        }
        return Ok(());
    }
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
            .with_title(APP_NAME)
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
    eframe::run_native(APP_NAME, opts, Box::new(|cc| Ok(Box::new(ui::Gui::new(cc)))))
}
