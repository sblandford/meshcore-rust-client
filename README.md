# Rust Desktop Client for MeshCore

A small, fast native desktop client for [MeshCore](https://github.com/meshcore-dev/MeshCore)
companion radios. It runs on Linux, Windows and macOS, and connects over **USB serial** or **Bluetooth LE**.

> **Independent project.** This is open-source software compatible with MeshCore radios. It is not affiliated
> with or endorsed by the MeshCore project, and is not the official MeshCore app. "MeshCore" is used only to
> identify the radios it works with.

## Features

- **Channels:** public, `#hashtag` (key derived from the name), private (random key) or joined with a key or link.
  You can add, edit, remove and share channels as `meshcore://channel/add?...` links.
- **Direct messages:** delivery tracking (sent ✓, delivered ✓✓ with round-trip time). Unacknowledged messages
  are retried automatically up to three times; the path is reset before the last attempt so it is flooded.
  Failed messages can be sent again.
- **Repeats:** your sent messages show how many relays were heard re-broadcasting them, with each relay's name
  and SNR. Channel posts are matched exactly by decrypting the overheard packets with your channel keys.
- **Room servers:** log in, read posts attributed to their original author, and post.
- **Repeaters:** log in as admin, remote CLI console, status (battery, noise floor, packet counters, airtime, uptime and so on),
  telemetry, **ping** (round-trip time and SNR both ways), trace route (out and back, with per-hop SNR),
  neighbour list, owner info and path discovery.
- **Contacts:** list with type filter, search, favourites and last-heard time. Reset path, set a route manually,
  see which route the last advert arrived by, share by zero-hop advert, copy or import `meshcore://` contact
  links, copy the public key, remove.
- **Discovered nodes:** nodes heard advertising but not added (manual mode, hop limit or full list) are listed
  separately and can be added with one click.
- **Contact auto-add:** add every node, or only chosen types; overwrite the oldest non-favourite contact when the
  list is full; limit auto-adding by hop count.
- **Node settings:** name, location, location in adverts, manual contact adding, telemetry sharing, extra ACKs.
- **Radio settings:** frequency, bandwidth, spreading factor, coding rate, TX power, client repeat mode,
  path hash size, region (flood scope) and advanced tuning (receive delay, airtime factor).
- **Device:** model and firmware, battery, storage, core/radio/packet statistics, clock check and sync, the
  radio's own telemetry, sensor settings (such as GPS), local command console (firmware v1.18+), BLE PIN,
  private key export and import, reboot and factory reset.
- **Emoji and other scripts:** modern emoji are drawn with a bundled monochrome Noto Emoji. Text in other
  scripts (Chinese, Japanese, Korean, Arabic, Indic, Thai and so on) uses your system's fonts, which are loaded
  only when such text appears.
- **Alerts:** messages that mention you (`@[your name]`) are highlighted, and the taskbar flashes when a message
  arrives while the window is in the background.
- **Right-click menus:**
  - Messages: copy, reply (`@[name]`), send again, message details (times, route, SNR, delivery and round trip),
    message the author directly, block the sender, delete.
  - Contacts and channels in the sidebar or the chat title: mark as read/unread, plus all contact or channel actions.
  - Log lines and status/telemetry tables: copy.
- **Blocking:** contacts are blocked by key; channel and room-server senders by name. Blocked messages are dropped
  on arrival and hidden. Manage the list in Settings › Blocked.
- Zero-hop and flood adverts, a received-packet log (SNR/RSSI and packet type), and an event log.
- Message history is kept locally per radio (radios only queue messages until they are synced).
  It is stored in `~/.local/share/meshcore-client` (Linux), `~/Library/Application Support/meshcore-client` (macOS)
  or `%APPDATA%\meshcore-client` (Windows).

## Installing

Installers are on the [Releases page](https://github.com/sblandford/meshcore-rust-client/releases/latest):
- **Windows:** `.msi`
- **macOS:** universal `.dmg`
- **Linux:** `.flatpak`

Portable archives are also provided. See **[INSTALL.md](INSTALL.md)** for step-by-step instructions.

## Building

```sh
cargo build --release        # binary: target/release/meshcore-rust-client (~7 MB, stripped)
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
meshcore-rust-client --scan                      # list serial ports and MeshCore BLE radios
meshcore-rust-client --probe /dev/ttyACM0        # read-only connection test (device info, channels, contacts)
meshcore-rust-client --probe <BLE id> [PIN]      # BLE id as shown by --scan
meshcore-rust-client --check-fonts "text"        # which system fonts are needed to draw some text
```

`tools/radio_sim.py` simulates a companion radio on a pseudo-terminal. It prints the pty path, which you can pass
to `--probe`; the GUI's port list only shows USB devices. Use it to try the client without hardware.

## Radio regulations

The app lets you set any frequency and transmit power your radio supports. You are responsible for using
settings (frequency, power and duty cycle) that are legal where you are.

## Acknowledgements

The companion-radio protocol is implemented from the [MeshCore firmware](https://github.com/meshcore-dev/MeshCore)
(MIT licence, © Scott Powell / rippleradios.com) and [meshcore.js](https://github.com/meshcore-dev/meshcore.js)
(MIT licence). Thanks to the MeshCore developers and community.

## Licence

GNU General Public License v3.0; see [LICENSE](LICENSE).

The app uses open-source libraries and fonts under their own licences, listed with their full texts in
[THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt). That file is included in every package. Regenerate it with
`python3 tools/third_party_notices.py` after changing dependencies; the release workflow fails if it is out of date.
