//! Bluetooth LE on macOS via CoreBluetooth. Delegate callbacks arrive on a private
//! dispatch queue and are forwarded over a channel to the link thread.

use super::{Found, LinkEvent, Notify, NUS_RX, NUS_SERVICE, NUS_TX};
use dispatch2::{DispatchQueue, DispatchQueueAttr};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_core_bluetooth::*;
use objc2_foundation::{NSArray, NSData, NSDictionary, NSError, NSNumber, NSString, NSUUID};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

enum Ev {
    State(CBManagerState),
    Found(Found),
    Connected,
    Failed(String),
    Disconnected(String),
    Services(Option<String>),
    Chars(Option<String>),
    Notifying(Option<String>),
    Value(Vec<u8>),
    Wrote(Option<String>),
}

struct Ivars {
    tx: Mutex<Sender<Ev>>,
}

fn err_text(e: Option<&NSError>) -> Option<String> {
    e.map(|e| e.localizedDescription().to_string())
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "MeshCoreBleDelegate"]
    #[ivars = Ivars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl CBCentralManagerDelegate for Delegate {
        #[unsafe(method(centralManagerDidUpdateState:))]
        fn did_update_state(&self, central: &CBCentralManager) {
            self.send(Ev::State(unsafe { central.state() }));
        }

        #[unsafe(method(centralManager:didDiscoverPeripheral:advertisementData:RSSI:))]
        fn did_discover(&self, _c: &CBCentralManager, p: &CBPeripheral, adv: &NSDictionary<NSString, AnyObject>, rssi: &NSNumber) {
            let mut name = unsafe { p.name() }.map(|n| n.to_string()).unwrap_or_default();
            let mut has_nus = false;
            unsafe {
                if let Some(n) = adv.objectForKey(CBAdvertisementDataLocalNameKey) {
                    if let Ok(s) = n.downcast::<NSString>() {
                        name = s.to_string();
                    }
                }
                if let Some(u) = adv.objectForKey(CBAdvertisementDataServiceUUIDsKey) {
                    if let Ok(arr) = u.downcast::<NSArray>() {
                        for x in arr.iter() {
                            if let Ok(id) = x.downcast::<CBUUID>() {
                                has_nus |= id.UUIDString().to_string().eq_ignore_ascii_case(NUS_SERVICE);
                            }
                        }
                    }
                }
            }
            if has_nus || name.starts_with("MeshCore") {
                let id = unsafe { p.identifier() }.UUIDString().to_string();
                self.send(Ev::Found(Found { id, name, rssi: Some(rssi.shortValue()), paired: false }));
            }
        }

        #[unsafe(method(centralManager:didConnectPeripheral:))]
        fn did_connect(&self, _c: &CBCentralManager, _p: &CBPeripheral) {
            self.send(Ev::Connected);
        }

        #[unsafe(method(centralManager:didFailToConnectPeripheral:error:))]
        fn did_fail(&self, _c: &CBCentralManager, _p: &CBPeripheral, e: Option<&NSError>) {
            self.send(Ev::Failed(err_text(e).unwrap_or_else(|| "connection failed".into())));
        }

        #[unsafe(method(centralManager:didDisconnectPeripheral:error:))]
        fn did_disconnect(&self, _c: &CBCentralManager, _p: &CBPeripheral, e: Option<&NSError>) {
            self.send(Ev::Disconnected(err_text(e).unwrap_or_else(|| "device disconnected".into())));
        }
    }

    unsafe impl CBPeripheralDelegate for Delegate {
        #[unsafe(method(peripheral:didDiscoverServices:))]
        fn did_services(&self, _p: &CBPeripheral, e: Option<&NSError>) {
            self.send(Ev::Services(err_text(e)));
        }

        #[unsafe(method(peripheral:didDiscoverCharacteristicsForService:error:))]
        fn did_chars(&self, _p: &CBPeripheral, _s: &CBService, e: Option<&NSError>) {
            self.send(Ev::Chars(err_text(e)));
        }

        #[unsafe(method(peripheral:didUpdateNotificationStateForCharacteristic:error:))]
        fn did_notify_state(&self, _p: &CBPeripheral, _c: &CBCharacteristic, e: Option<&NSError>) {
            self.send(Ev::Notifying(err_text(e)));
        }

        #[unsafe(method(peripheral:didUpdateValueForCharacteristic:error:))]
        fn did_value(&self, _p: &CBPeripheral, c: &CBCharacteristic, e: Option<&NSError>) {
            if e.is_none() {
                if let Some(d) = unsafe { c.value() } {
                    self.send(Ev::Value(d.to_vec()));
                }
            }
        }

        #[unsafe(method(peripheral:didWriteValueForCharacteristic:error:))]
        fn did_write(&self, _p: &CBPeripheral, _c: &CBCharacteristic, e: Option<&NSError>) {
            self.send(Ev::Wrote(err_text(e)));
        }
    }
);

impl Delegate {
    fn new(tx: Sender<Ev>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars { tx: Mutex::new(tx) });
        unsafe { msg_send![super(this), init] }
    }
    fn send(&self, ev: Ev) {
        let _ = self.ivars().tx.lock().unwrap().send(ev);
    }
}

struct Central {
    mgr: Retained<CBCentralManager>,
    delegate: Retained<Delegate>,
    events: Receiver<Ev>,
    _queue: dispatch2::DispatchRetained<DispatchQueue>,
}

impl Central {
    fn new() -> Result<Central, String> {
        let (tx, events) = channel();
        let delegate = Delegate::new(tx);
        let queue = DispatchQueue::new("meshcore.ble", DispatchQueueAttr::SERIAL);
        let mgr = unsafe {
            CBCentralManager::initWithDelegate_queue(CBCentralManager::alloc(), Some(ProtocolObject::from_ref(&*delegate)), Some(&queue))
        };
        let c = Central { mgr, delegate, events, _queue: queue };
        let t0 = Instant::now();
        loop {
            match c.events.recv_timeout(Duration::from_millis(200)) {
                Ok(Ev::State(s)) if s == CBManagerState::PoweredOn => return Ok(c),
                Ok(Ev::State(s)) if s == CBManagerState::Unauthorized => {
                    return Err("Bluetooth permission denied – allow it in System Settings › Privacy & Security › Bluetooth".into())
                }
                Ok(Ev::State(s)) if s == CBManagerState::PoweredOff => return Err("Bluetooth is switched off".into()),
                Ok(Ev::State(s)) if s == CBManagerState::Unsupported => return Err("Bluetooth LE is not supported on this Mac".into()),
                _ if t0.elapsed() > Duration::from_secs(10) => return Err("Bluetooth did not become ready".into()),
                _ => {}
            }
        }
    }

    /// Wait for an event selected by `f` (which returns Some to finish).
    fn wait<T>(&self, secs: u64, stop: &AtomicBool, mut f: impl FnMut(Ev) -> Option<Result<T, String>>) -> Result<T, String> {
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_secs(secs) {
            if super::stopped(stop) {
                return Err("cancelled".into());
            }
            if let Ok(ev) = self.events.recv_timeout(Duration::from_millis(100)) {
                if let Ev::Disconnected(e) | Ev::Failed(e) = &ev {
                    return Err(e.clone());
                }
                if let Some(r) = f(ev) {
                    return r;
                }
            }
        }
        Err("timed out".into())
    }
}

pub fn scan(results: Arc<Mutex<Vec<Found>>>, stop: &AtomicBool, wake: Arc<dyn Fn() + Send + Sync>) -> Result<(), String> {
    let c = Central::new()?;
    unsafe { c.mgr.scanForPeripheralsWithServices_options(None, None) };
    while !super::stopped(stop) {
        if let Ok(Ev::Found(f)) = c.events.recv_timeout(Duration::from_millis(200)) {
            if super::upsert(&results, f) {
                wake();
            }
        }
    }
    unsafe { c.mgr.stopScan() };
    Ok(())
}

fn uuid_is(c: &CBAttribute, want: &str) -> bool {
    unsafe { c.UUID().UUIDString() }.to_string().eq_ignore_ascii_case(want)
}

pub fn run(id: &str, _pin: u32, rx: Receiver<Vec<u8>>, stop: &AtomicBool, notify: &Notify) -> Result<(), String> {
    let status = |s: &str| notify(LinkEvent::Status(s.into()));
    let c = Central::new()?;
    let uuid = NSUUID::initWithUUIDString(NSUUID::alloc(), &NSString::from_str(id)).ok_or("bad device id")?;
    let list = unsafe { c.mgr.retrievePeripheralsWithIdentifiers(&NSArray::from_retained_slice(&[uuid])) };
    let p = list.firstObject().ok_or("device not known to macOS – scan for it first")?;
    unsafe { p.setDelegate(Some(ProtocolObject::from_ref(&*c.delegate))) };

    status("Connecting…");
    unsafe { c.mgr.connectPeripheral_options(&p, None) };
    let res = (|| {
        c.wait(30, stop, |ev| matches!(ev, Ev::Connected).then_some(Ok(())))?;
        status("Discovering services…");
        let svc_uuid = unsafe { CBUUID::UUIDWithString(&NSString::from_str(NUS_SERVICE)) };
        unsafe { p.discoverServices(Some(&NSArray::from_retained_slice(&[svc_uuid]))) };
        c.wait(20, stop, |ev| match ev {
            Ev::Services(None) => Some(Ok(())),
            Ev::Services(Some(e)) => Some(Err(e)),
            _ => None,
        })?;
        let svc = unsafe { p.services() }
            .and_then(|s| s.iter().find(|s| uuid_is(s, NUS_SERVICE)))
            .ok_or("device has no MeshCore (Nordic UART) service")?;
        unsafe { p.discoverCharacteristics_forService(None, &svc) };
        c.wait(20, stop, |ev| match ev {
            Ev::Chars(None) => Some(Ok(())),
            Ev::Chars(Some(e)) => Some(Err(e)),
            _ => None,
        })?;
        let chars = unsafe { svc.characteristics() }.ok_or("no characteristics")?;
        let rx_c = chars.iter().find(|c| uuid_is(c, NUS_RX)).ok_or("MeshCore RX characteristic missing")?;
        let tx_c = chars.iter().find(|c| uuid_is(c, NUS_TX)).ok_or("MeshCore TX characteristic missing")?;
        // Subscribing triggers macOS's own pairing prompt (PIN entry) if the radio requires it.
        unsafe { p.setNotifyValue_forCharacteristic(true, &tx_c) };
        c.wait(60, stop, |ev| match ev {
            Ev::Notifying(None) => Some(Ok(())),
            Ev::Notifying(Some(e)) => Some(Err(format!("cannot subscribe: {e}"))),
            _ => None,
        })?;
        notify(LinkEvent::Connected);
        loop {
            if super::stopped(stop) {
                return Ok(());
            }
            for f in super::drain(&rx) {
                let data = NSData::with_bytes(&f);
                unsafe { p.writeValue_forCharacteristic_type(&data, &rx_c, CBCharacteristicWriteType::WithResponse) };
            }
            match c.events.recv_timeout(Duration::from_millis(20)) {
                Ok(Ev::Value(v)) => notify(LinkEvent::Frame(v)),
                Ok(Ev::Disconnected(e)) => return Err(e),
                Ok(Ev::Wrote(Some(e))) => status(&format!("Write failed: {e}")),
                _ => {}
            }
        }
    })();
    unsafe { c.mgr.cancelPeripheralConnection(&p) };
    res
}
