# Installing the MeshCore desktop client

Download the file for your system from the
[Releases page](https://github.com/sblandford/meshcore-rust-client/releases/latest).
Replace `vX.Y.Z` below with the version you downloaded.

| System | Installer | Portable (no install) |
|---|---|---|
| Windows 10/11, 64-bit | `meshcore-vX.Y.Z-windows-x86_64.msi` | `meshcore-vX.Y.Z-windows-x86_64.zip` |
| macOS 11 or later, Apple Silicon or Intel | `meshcore-vX.Y.Z-macos-universal.dmg` | |
| Linux, 64-bit | `meshcore-vX.Y.Z-linux-x86_64.flatpak` | `meshcore-vX.Y.Z-linux-x86_64.tar.gz` |

To check a download wasn't corrupted, compare its SHA-256 checksum with `SHA256SUMS.txt`:

| System | Command |
|---|---|
| Linux | `sha256sum -c SHA256SUMS.txt --ignore-missing` |
| macOS | `shasum -a 256 <file>` |
| Windows (PowerShell) | `Get-FileHash <file>` |

The packages are not signed with a paid code-signing certificate, so Windows and macOS warn you the first time.
The steps below explain how to get past the warnings.

---

## Windows

### Install
1. Double-click `meshcore-vX.Y.Z-windows-x86_64.msi`.
2. If **"Windows protected your PC"** (SmartScreen) appears, click **More info**, then **Run anyway**.
3. Follow the installer: accept the licence, choose a folder (the default is `C:\Program Files\MeshCore`),
   then click **Install**. Windows asks for administrator permission.
4. Start **MeshCore** from the Start menu.

To upgrade, run the newer `.msi`. It replaces the old version, and your message history is kept.

### Uninstall
Go to **Settings › Apps › Installed apps › MeshCore › Uninstall**. Message history stays in
`%APPDATA%\meshcore-client`. Delete that folder as well if you want to remove it.

### Portable version
Extract the `.zip` anywhere and run `meshcore.exe`. Nothing is installed.

### Connecting
- **USB:** Windows 10/11 include drivers for most MeshCore boards. Boards with CP210x or CH340 USB chips
  may need the chip maker's driver. The radio appears as a `COM` port in the Connect dialog.
- **Bluetooth:** click **Scan**, pick the radio, and enter its PIN if asked. The PIN is shown on the radio's
  screen; the default is `123456`.

---

## macOS

### Install
1. Open `meshcore-vX.Y.Z-macos-universal.dmg`.
2. Drag **MeshCore** onto the **Applications** folder in the same window, then eject the disk image.
3. Open MeshCore from Applications or Launchpad. The first time, macOS blocks it because the app isn't
   notarised by Apple:
   - **macOS 15 (Sequoia) or later:** click **Done**. Then open **System Settings › Privacy & Security**,
     scroll down to the MeshCore message, click **Open Anyway** and confirm with your password.
   - **macOS 14 or earlier:** right-click (or Control-click) MeshCore in Applications, choose **Open**,
     then click **Open**.

   Alternatively, run this once in Terminal:
   ```sh
   xattr -dr com.apple.quarantine /Applications/MeshCore.app
   ```
4. When asked whether MeshCore may use Bluetooth, click **Allow**.

To upgrade, drag the newer version into Applications and choose **Replace**.

### Uninstall
Drag **MeshCore** from Applications to the Bin. Message history stays in
`~/Library/Application Support/meshcore-client`.

### Connecting
- **USB:** most boards appear as `/dev/cu.usbmodem…` and need no driver. Boards with CH340 or CP210x USB chips
  may need the chip maker's driver.
- **Bluetooth:** click **Scan** and pick the radio. macOS shows its own pairing prompt if the radio needs a PIN
  (the default is `123456`). If you turned Bluetooth access off earlier, re-enable it in
  **System Settings › Privacy & Security › Bluetooth**.

---

## Linux

### Flatpak (recommended)
Flatpak works on almost every distribution. If you don't have it yet, see
[flatpak.org/setup](https://flatpak.org/setup/).

```sh
# The app runs on the Freedesktop runtime from Flathub, which is fetched automatically:
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user meshcore-vX.Y.Z-linux-x86_64.flatpak
```

Start **MeshCore** from your applications menu, or run:
```sh
flatpak run io.github.sblandford.MeshCoreClient
```

To upgrade, install the newer `.flatpak` file the same way.

To uninstall:
```sh
flatpak uninstall io.github.sblandford.MeshCoreClient                 # keeps message history
flatpak uninstall --delete-data io.github.sblandford.MeshCoreClient   # also deletes it
```

The Flatpak has these permissions:

| Permission | Why |
|---|---|
| Display (Wayland or X11) with GPU | The window and its OpenGL drawing. |
| All devices (`--device=all`) | Opening the USB serial port. Flatpak has no narrower permission for serial ports. |
| Talking to BlueZ on the system bus | Bluetooth. |

The app has no network access and no access to your files. Its message history is stored in
`~/.var/app/io.github.sblandford.MeshCoreClient/data/meshcore-client`.

### Portable tar.gz
```sh
tar xzf meshcore-vX.Y.Z-linux-x86_64.tar.gz
cd meshcore-vX.Y.Z-linux-x86_64
./meshcore
```

Optionally, add it to your applications menu:
```sh
install -Dm755 meshcore ~/.local/bin/meshcore
install -Dm644 icons/io.github.sblandford.MeshCoreClient.png ~/.local/share/icons/hicolor/256x256/apps/io.github.sblandford.MeshCoreClient.png
sed "s|^Exec=.*|Exec=$HOME/.local/bin/meshcore|" io.github.sblandford.MeshCoreClient.desktop > ~/.local/share/applications/io.github.sblandford.MeshCoreClient.desktop
```

The binary needs glibc 2.35 or newer (Ubuntu 22.04, Debian 12, Fedora 36 or later). It loads X11 or Wayland and
OpenGL at run time; normal desktop systems already have them.

### Connecting
- **USB:** your user must be allowed to open serial ports. On most distributions, run the command below, then
  log out and back in (on Arch, use the group `uucp` instead of `dialout`):
  ```sh
  sudo usermod -aG dialout $USER
  ```
  This applies to the Flatpak as well.
- **Bluetooth:** needs BlueZ (the standard Linux Bluetooth service) to be running. Click **Scan**, pick the radio,
  and enter its PIN if it hasn't been paired before (the default is `123456`).

---

## Building the packages yourself

See [README.md](README.md) for building from source. Each package can also be built locally:

### Flatpak
Install the builder and SDK, then build from the repository root:
```sh
flatpak install --user flathub org.flatpak.Builder org.freedesktop.Sdk//26.08 org.freedesktop.Sdk.Extension.rust-stable//26.08
flatpak run org.flatpak.Builder --user --install --force-clean --disable-rofiles-fuse --repo=flatpak-repo \
    build-dir packaging/linux/io.github.sblandford.MeshCoreClient.yml
flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
    flatpak-repo meshcore.flatpak io.github.sblandford.MeshCoreClient master
```

### Windows MSI and macOS DMG
Follow the same steps as the `windows` and `macos` jobs in `.github/workflows/release.yml`:
- **Windows** uses WiX Toolset 5.
- **macOS** uses `lipo`, `codesign` and `hdiutil`, which come with Xcode's command-line tools.
