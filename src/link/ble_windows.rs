//! Bluetooth LE on Windows via the WinRT `Windows.Devices.Bluetooth` API.

use super::{Found, LinkEvent, Notify};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows::core::{Ref, GUID, HSTRING};
use windows::Devices::Bluetooth::Advertisement::*;
use windows::Devices::Bluetooth::GenericAttributeProfile::*;
use windows::Devices::Bluetooth::{BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice};
use windows::Devices::Enumeration::{DeviceInformationCustomPairing, DevicePairingKinds, DevicePairingRequestedEventArgs, DevicePairingResultStatus};
use windows::Foundation::TypedEventHandler;
use windows::Storage::Streams::{DataReader, DataWriter};

const NUS: GUID = GUID::from_u128(0x6e400001_b5a3_f393_e0a9_e50e24dcca9e);
const RX: GUID = GUID::from_u128(0x6e400002_b5a3_f393_e0a9_e50e24dcca9e);
const TX: GUID = GUID::from_u128(0x6e400003_b5a3_f393_e0a9_e50e24dcca9e);

fn e(err: windows::core::Error) -> String {
    let m = err.message();
    if m.is_empty() {
        format!("{err:?}")
    } else {
        m
    }
}

fn addr_str(a: u64) -> String {
    (0..6).rev().map(|i| format!("{:02X}", (a >> (i * 8)) & 0xFF)).collect::<Vec<_>>().join(":")
}

pub fn scan(results: Arc<Mutex<Vec<Found>>>, stop: &AtomicBool, wake: Arc<dyn Fn() + Send + Sync>) -> Result<(), String> {
    let watcher = BluetoothLEAdvertisementWatcher::new().map_err(e)?;
    watcher.SetScanningMode(BluetoothLEScanningMode::Active).map_err(e)?;
    let handler = TypedEventHandler::<BluetoothLEAdvertisementWatcher, BluetoothLEAdvertisementReceivedEventArgs>::new(
        move |_, args: Ref<BluetoothLEAdvertisementReceivedEventArgs>| {
            let a = args.ok()?;
            let adv = a.Advertisement()?;
            let name = adv.LocalName().map(|n| n.to_string()).unwrap_or_default();
            let has_nus = adv.ServiceUuids().map(|u| u.into_iter().any(|g| g == NUS)).unwrap_or(false);
            if has_nus || name.starts_with("MeshCore") {
                let f = Found { id: addr_str(a.BluetoothAddress()?), name, rssi: a.RawSignalStrengthInDBm().ok(), paired: false };
                if super::upsert(&results, f) {
                    wake();
                }
            }
            Ok(())
        },
    );
    let token = watcher.Received(&handler).map_err(e)?;
    watcher.Start().map_err(|err| format!("cannot start scanning (is Bluetooth on?): {}", e(err)))?;
    while !super::stopped(stop) {
        std::thread::sleep(Duration::from_millis(200));
    }
    let _ = watcher.Stop();
    let _ = watcher.RemoveReceived(token);
    Ok(())
}

pub fn run(id: &str, pin: u32, rx: Receiver<Vec<u8>>, stop: &AtomicBool, notify: &Notify) -> Result<(), String> {
    let status = |s: &str| notify(LinkEvent::Status(s.into()));
    let addr = u64::from_str_radix(&id.replace(':', ""), 16).map_err(|_| format!("bad Bluetooth address {id}"))?;
    status("Connecting…");
    let dev = BluetoothLEDevice::FromBluetoothAddressAsync(addr).map_err(e)?.join().map_err(|err| format!("device not found: {}", e(err)))?;

    // Pair with the PIN if Windows doesn't already have a bond.
    let pairing = dev.DeviceInformation().and_then(|d| d.Pairing()).map_err(e)?;
    if !pairing.IsPaired().unwrap_or(false) && pairing.CanPair().unwrap_or(false) {
        status("Pairing…");
        let custom: DeviceInformationCustomPairing = pairing.Custom().map_err(e)?;
        let pin_h = HSTRING::from(format!("{pin:06}"));
        let tok = custom
            .PairingRequested(&TypedEventHandler::<DeviceInformationCustomPairing, DevicePairingRequestedEventArgs>::new(
                move |_, args: Ref<DevicePairingRequestedEventArgs>| {
                    let a = args.ok()?;
                    if a.PairingKind()? == DevicePairingKinds::ProvidePin {
                        a.AcceptWithPin(&pin_h)
                    } else {
                        a.Accept()
                    }
                },
            ))
            .map_err(e)?;
        let kinds = DevicePairingKinds::ConfirmOnly | DevicePairingKinds::ProvidePin | DevicePairingKinds::ConfirmPinMatch;
        let res = custom.PairAsync(kinds).map_err(e)?.join().map_err(e)?;
        let _ = custom.RemovePairingRequested(tok);
        let st = res.Status().map_err(e)?;
        if st != DevicePairingResultStatus::Paired && st != DevicePairingResultStatus::AlreadyPaired {
            return Err(format!("pairing failed ({st:?}) – check the PIN"));
        }
    }

    status("Discovering services…");
    let sr = dev.GetGattServicesForUuidWithCacheModeAsync(NUS, BluetoothCacheMode::Uncached).map_err(e)?.join().map_err(e)?;
    if sr.Status().map_err(e)? != GattCommunicationStatus::Success {
        return Err(format!("cannot read services ({:?}) – is the radio in range?", sr.Status().ok()));
    }
    let svc = sr.Services().map_err(e)?.GetAt(0).map_err(|_| "device has no MeshCore (Nordic UART) service".to_string())?;
    let get_char = |uuid: GUID| -> Result<GattCharacteristic, String> {
        let r = svc.GetCharacteristicsForUuidAsync(uuid).map_err(e)?.join().map_err(e)?;
        r.Characteristics().map_err(e)?.GetAt(0).map_err(|_| "MeshCore characteristic missing".to_string())
    };
    let rx_char = get_char(RX)?;
    let tx_char = get_char(TX)?;

    let n2 = notify.clone();
    let vtok = tx_char
        .ValueChanged(&TypedEventHandler::<GattCharacteristic, GattValueChangedEventArgs>::new(move |_, args: Ref<GattValueChangedEventArgs>| {
            let buf = args.ok()?.CharacteristicValue()?;
            let mut v = vec![0u8; buf.Length()? as usize];
            DataReader::FromBuffer(&buf)?.ReadBytes(&mut v)?;
            n2(LinkEvent::Frame(v));
            Ok(())
        }))
        .map_err(e)?;
    let st = tx_char
        .WriteClientCharacteristicConfigurationDescriptorAsync(GattClientCharacteristicConfigurationDescriptorValue::Notify)
        .map_err(e)?
        .join()
        .map_err(e)?;
    if st != GattCommunicationStatus::Success {
        let _ = tx_char.RemoveValueChanged(vtok);
        return Err(format!("cannot subscribe to notifications ({st:?})"));
    }
    notify(LinkEvent::Connected);

    let result = loop {
        if super::stopped(stop) {
            break Ok(());
        }
        if dev.ConnectionStatus().map(|s| s == BluetoothConnectionStatus::Disconnected).unwrap_or(false) {
            break Err("device disconnected".to_string());
        }
        let mut failed = None;
        for f in super::drain(&rx) {
            let w = DataWriter::new().map_err(e)?;
            w.WriteBytes(&f).map_err(e)?;
            let buf = w.DetachBuffer().map_err(e)?;
            match rx_char.WriteValueWithResultAndOptionAsync(&buf, GattWriteOption::WriteWithResponse).and_then(|op| op.join()) {
                Ok(r) if r.Status().ok() == Some(GattCommunicationStatus::Success) => {}
                Ok(r) => status(&format!("Write failed: {:?}", r.Status().ok())),
                Err(err) => {
                    failed = Some(format!("write failed: {}", e(err)));
                    break;
                }
            }
        }
        if let Some(f) = failed {
            break Err(f);
        }
        std::thread::sleep(Duration::from_millis(15));
    };
    let _ = tx_char.RemoveValueChanged(vtok);
    let _ = dev.Close();
    result
}
