# MeshCore desktop client

A small, fast native desktop client for [MeshCore](https://github.com/meshcore-dev/MeshCore)
companion radios. It runs on Linux, Windows and macOS, and connects over **USB serial** or **Bluetooth LE**.

## Features

- **Channels:** public, `#hashtag` (key derived from the name), private (random key) or joined with a key or link.
  You can add, edit, remove and share channels as `meshcore://channel/add?...` links.
- **Direct messages:** delivery tracking (sent ✓, delivered ✓✓ with round-trip time). Unacknowledged messages
  are retried automatically up to three times; the path is reset before the last attempt so it is flooded.
  Failed messages can be sent again.
- **Room servers:** log in, read posts attributed to their original author, and post.
- **Repeaters:** log in as admin, remote CLI console, status (battery, noise floor, packet counters, airtime, uptime and so on),
  telemetry, trace route (out and back, with per-hop SNR) and path discovery.
- **Contacts:** list with type filter, search, favourites and last-heard time. Reset path, share by zero-hop advert,
  copy or import `meshcore://` contact links, copy the public key, remove.
- **Node settings:** name, location, location in adverts, manual contact adding, telemetry sharing, extra ACKs.
- **Radio settings:** frequency, bandwidth, spreading factor, coding rate, TX power.
- **Device:** model and firmware, battery, storage, core/radio/packet statistics, clock sync, BLE PIN, reboot.
- Zero-hop and flood adverts, a received-packet log (SNR/RSSI and packet type), and an event log.
- Message history is kept locally per radio (radios only queue messages until they are synced).
  It is stored in `~/.local/share/meshcore-client` (Linux), `~/Library/Application Support/meshcore-client` (macOS)
  or `%APPDATA%\meshcore-client` (Windows).

## Building

```sh
cargo build --release        # binary: target/release/meshcore (~7 MB, stripped)
```

You need Rust 1.92 or newer. There are no system library build dependencies on any platform. On Linux, X11/Wayland
and OpenGL are loaded at run time, and the binary links only against libc.

## Dependencies (deliberately few)

| Purpose | Linux | Windows | macOS |
|---|---|---|---|
| GUI | `eframe`/`egui` (OpenGL "glow" backend, no wgpu) | same | same |
| USB serial | hand-written termios via `libc` | hand-written Win32 via `windows` | termios via `libc` |
| Bluetooth LE | **hand-written minimal D-Bus client to BlueZ** (no libdbus, btleplug or tokio) | WinRT via `windows` | CoreBluetooth via `objc2-core-bluetooth` |

SHA-256, hex and base64, Cayenne LPP decoding, the local-time conversion, persistence and the MeshCore protocol
itself are all implemented in-tree.

## Platform notes

- **Linux:** BLE needs BlueZ (the `bluetoothd` service). USB serial needs read/write access to `/dev/ttyACM*` or
  `/dev/ttyUSB*`; add yourself to the `dialout` group (or `uucp` on some distributions). If the radio isn't
  bonded yet, the app pairs it using the PIN you enter (default `123456`).
- **Windows:** pairing uses the PIN from the connect dialog.
- **macOS:** the system shows its own pairing prompt. If you bundle the app as an `.app`, add
  `NSBluetoothAlwaysUsageDescription` to `Info.plist`.
- Only one application should talk to the radio at a time. If another app (for example the web client in
  Chrome) already holds the BLE connection, the app warns you, because replies would be shared.

## Diagnostics

```sh
meshcore --scan                      # list serial ports and MeshCore BLE radios
meshcore --probe /dev/ttyACM0        # read-only connection test (device info, channels, contacts)
meshcore --probe <BLE id> [PIN]      # BLE id as shown by --scan
```

`tools/radio_sim.py` simulates a companion radio on a pseudo-terminal. It prints the pty path, which you can pass
to `--probe`; the GUI's port list only shows USB devices. Use it to try the client without hardware.

## Licence

GNU General Public License v3.0 – see [LICENSE](LICENSE).
