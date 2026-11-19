//! Application state and protocol logic (independent of the GUI).

use crate::link::{Link, LinkEvent, Notify, Target};
use crate::proto::{self, Contact, DeviceInfo, Frame, SelfInfo};
use crate::store::{Settings, Store};
use crate::util::{self, hex, now};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ChatKey {
    Channel([u8; 16]),
    Contact([u8; 32]),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Status {
    Received,
    Pending,
    Sent,
    Delivered(u32),
    Failed,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub id: u64,
    pub out: bool,
    /// Sender's timestamp (unix seconds).
    pub ts: u32,
    /// Local receive/send time.
    pub recv: u64,
    pub author: String,
    pub text: String,
    pub status: Status,
    pub txt_type: u8,
    pub snr: Option<f32>,
    pub path_len: u8,
    pub ack: u32,
    pub attempt: u8,
    pub deadline: Option<Instant>,
    /// Relays heard re-transmitting this (outgoing) message: (relay hash, SNR).
    /// An empty hash means the relay could not be identified.
    pub repeats: Vec<(Vec<u8>, f32)>,
}

#[derive(Default)]
pub struct Chat {
    pub msgs: Vec<Message>,
    pub unread: u32,
}

impl Chat {
    pub fn last_time(&self) -> u64 {
        self.msgs.last().map(|m| m.recv).unwrap_or(0)
    }
}

#[derive(Clone, Debug)]
pub struct Channel {
    pub idx: u8,
    pub name: String,
    pub secret: [u8; 16],
}

/// Per-contact state for repeaters / room servers / sensors.
#[derive(Default)]
pub struct Remote {
    pub logged_in: Option<bool>,
    pub busy: Option<&'static str>,
    pub status: Vec<(String, String)>,
    pub status_at: u64,
    pub telemetry: Vec<(String, String)>,
    pub telemetry_at: u64,
    pub trace: Option<String>,
    pub discovery: Option<String>,
    pub ping: Option<String>,
    pub neighbours: Option<(u16, Vec<proto::Neighbour>, u64)>,
    pub owner: Option<String>,
    pub advert_path: Option<String>,
}

/// Kinds of binary request sent to repeaters.
pub const BIN_NEIGHBOURS: u8 = 1;
pub const BIN_OWNER: u8 = 2;

struct TraceReq {
    pk: [u8; 32],
    ping: bool,
    sent: Instant,
}

#[derive(PartialEq, Clone, Debug)]
pub enum Conn {
    Disconnected,
    Connecting(String),
    Connected,
}

/// What a queued command's reply means to us.
#[derive(Debug)]
enum Expect {
    Quiet,
    Label(String),
    DeviceInfo,
    SelfInfo,
    Contacts,
    OneContact,
    Channel(u8),
    Sync,
    Batt,
    Dm(ChatKey, u64),
    ChannelMsg(ChatKey, u64),
    Login([u8; 32]),
    Remote([u8; 32], &'static str),
    Trace([u8; 32]),
    Export(String),
    Stats(u8),
    Reload(String),
    DeviceRefresh,
    AutoAdd,
    Tuning,
    Scope,
    CustomVars,
    RepeatFreqs,
    DeviceTime,
    PrivateKey,
    SelfTelemetry,
    Cli,
    AdvertPath([u8; 32]),
    Binary([u8; 32], u8),
}

struct Cmd {
    frame: Vec<u8>,
    expect: Expect,
}

pub struct LogLine {
    pub t: u64,
    pub text: String,
}

const CMD_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_ATTEMPTS: u8 = 3;

pub struct App {
    // link
    link: Option<Link>,
    link_gen: u64,
    events: Receiver<(u64, LinkEvent)>,
    events_tx: std::sync::mpsc::Sender<(u64, LinkEvent)>,
    wake: Arc<dyn Fn() + Send + Sync>,
    pub conn: Conn,
    pub target: Option<Target>,
    queue: VecDeque<Cmd>,
    inflight: Option<(Expect, Instant)>,
    syncing: bool,
    last_poll: Instant,

    // device model
    pub device: Option<DeviceInfo>,
    pub me: Option<SelfInfo>,
    pub batt: Option<(u16, u32, u32)>,
    pub contacts: Vec<Contact>,
    pub channels: Vec<Channel>,
    pub chats: HashMap<ChatKey, Chat>,
    pub remotes: HashMap<[u8; 32], Remote>,
    pub stats: [Vec<(String, String)>; 3],
    store: Option<Store>,
    next_id: u64,
    awaiting_ack: Vec<(ChatKey, u64)>,
    traces: HashMap<u32, TraceReq>,
    binary_pending: HashMap<u32, ([u8; 32], u8)>,

    // optional device settings (None = not reported by this firmware)
    pub autoadd: Option<(u8, u8)>,
    pub tuning: Option<(u32, u32)>,
    pub default_scope: Option<Option<(String, [u8; 16])>>,
    pub custom_vars: Vec<(String, String)>,
    pub repeat_freqs: Vec<(u32, u32)>,
    /// (radio clock, local clock) when last read.
    pub device_clock: Option<(u32, u64)>,
    pub self_telemetry: Vec<(String, String)>,
    pub private_key: Option<Vec<u8>>,
    pub cli_log: VecDeque<String>,
    /// Nodes heard advertising that the radio did not add as contacts.
    pub discovered: Vec<Contact>,
    /// Set when a message arrives; the UI clears it after alerting the user.
    pub attention: bool,

    // log + notices
    pub log: VecDeque<LogLine>,
    pub rx_log: VecDeque<LogLine>,
    pub toasts: VecDeque<(String, Instant)>,
    pub settings: Settings,
    /// Contacts whose direct messages are dropped.
    pub blocked_keys: Vec<[u8; 32]>,
    /// Sender names whose channel / room-server posts are dropped.
    pub blocked_names: Vec<String>,
    pub selected: Option<ChatKey>,
    /// Text placed here should be copied to the clipboard by the UI.
    pub clipboard: Option<String>,
}

impl App {
    pub fn new(wake: Arc<dyn Fn() + Send + Sync>) -> App {
        let mut app = App::build(wake);
        app.blocked_keys = app
            .settings
            .get("blocked_keys")
            .unwrap_or("")
            .split(',')
            .filter_map(|h| util::unhex(h)?.try_into().ok())
            .collect();
        app.blocked_names = app.settings.get("blocked_names").unwrap_or("").split('\n').filter(|n| !n.is_empty()).map(str::to_string).collect();
        app
    }

    fn build(wake: Arc<dyn Fn() + Send + Sync>) -> App {
        let (events_tx, events) = channel();
        App {
            link: None,
            link_gen: 0,
            events,
            events_tx,
            wake,
            conn: Conn::Disconnected,
            target: None,
            queue: VecDeque::new(),
            inflight: None,
            syncing: false,
            last_poll: Instant::now(),
            device: None,
            me: None,
            batt: None,
            contacts: Vec::new(),
            channels: Vec::new(),
            chats: HashMap::new(),
            remotes: HashMap::new(),
            stats: Default::default(),
            store: None,
            next_id: 1,
            awaiting_ack: Vec::new(),
            traces: HashMap::new(),
            binary_pending: HashMap::new(),
            autoadd: None,
            tuning: None,
            default_scope: None,
            custom_vars: Vec::new(),
            repeat_freqs: Vec::new(),
            device_clock: None,
            self_telemetry: Vec::new(),
            private_key: None,
            cli_log: VecDeque::new(),
            discovered: Vec::new(),
            attention: false,
            log: VecDeque::new(),
            rx_log: VecDeque::new(),
            toasts: VecDeque::new(),
            settings: Settings::load(),
            blocked_keys: Vec::new(),
            blocked_names: Vec::new(),
            selected: None,
            clipboard: None,
        }
    }

    // ------------------------------------------------------------ connection

    pub fn connect(&mut self, t: Target) {
        self.disconnect();
        self.link_gen += 1;
        let gen = self.link_gen;
        let tx = self.events_tx.clone();
        let wake = self.wake.clone();
        let notify: Notify = Arc::new(move |e| {
            let _ = tx.send((gen, e));
            wake();
        });
        self.note(format!("Connecting to {}", t.label()));
        self.conn = Conn::Connecting("Starting…".into());
        self.link = Some(Link::open(t.clone(), notify));
        self.target = Some(t);
    }

    pub fn disconnect(&mut self) {
        if self.link.take().is_some() {
            self.note("Disconnected".into());
        }
        self.conn = Conn::Disconnected;
        self.queue.clear();
        self.inflight = None;
        self.syncing = false;
    }

    pub fn connected(&self) -> bool {
        self.conn == Conn::Connected
    }

    fn on_connected(&mut self) {
        self.conn = Conn::Connected;
        self.note("Link up, querying device".into());
        self.contacts.clear();
        self.channels.clear();
        self.push(proto::device_query(), Expect::DeviceInfo);
        self.push(proto::app_start(), Expect::SelfInfo);
        self.push(proto::set_time(now() as u32), Expect::Quiet);
        self.push(proto::simple(proto::CMD_GET_BATT_AND_STORAGE), Expect::Batt);
        self.push(proto::get_contacts(0), Expect::Contacts);
        // channels are requested once DEVICE_INFO tells us how many there are
    }

    // ------------------------------------------------------------ command queue

    fn push(&mut self, frame: Vec<u8>, expect: Expect) {
        if self.link.is_some() {
            self.queue.push_back(Cmd { frame, expect });
        }
    }

    fn pump(&mut self) {
        if !self.connected() {
            return;
        }
        if let Some((e, t)) = &self.inflight {
            if t.elapsed() < CMD_TIMEOUT {
                return;
            }
            let e = format!("{e:?}");
            self.note(format!("No reply to command ({e}), continuing"));
            if e == "Sync" {
                self.syncing = false;
            }
            self.inflight = None;
        }
        if let Some(c) = self.queue.pop_front() {
            if let Some(l) = &self.link {
                l.send(c.frame);
            }
            // Every queued command gets exactly one reply (or a contact stream).
            self.inflight = Some((c.expect, Instant::now()));
        }
    }

    /// Called every UI frame: process link events, timers and the queue.
    pub fn tick(&mut self) {
        while let Ok((gen, ev)) = self.events.try_recv() {
            if gen != self.link_gen {
                continue;
            }
            match ev {
                LinkEvent::Status(s) => {
                    if let Conn::Connecting(_) = self.conn {
                        self.conn = Conn::Connecting(s.clone());
                    }
                    self.note(s);
                }
                LinkEvent::Connected => self.on_connected(),
                LinkEvent::Frame(f) => self.on_frame(&f),
                LinkEvent::Closed(err) => {
                    self.link = None;
                    self.conn = Conn::Disconnected;
                    self.queue.clear();
                    self.inflight = None;
                    self.syncing = false;
                    match err {
                        Some(e) => {
                            self.note(format!("Connection closed: {e}"));
                            self.toast(format!("Connection lost: {e}"));
                        }
                        None => self.note("Connection closed".into()),
                    }
                }
            }
        }
        self.check_acks();
        if self.connected() && self.last_poll.elapsed() > Duration::from_secs(60) {
            self.last_poll = Instant::now();
            self.push(proto::simple(proto::CMD_GET_BATT_AND_STORAGE), Expect::Batt);
            if !self.syncing {
                self.syncing = true;
                self.push(proto::simple(proto::CMD_SYNC_NEXT_MESSAGE), Expect::Sync);
            }
        }
        self.pump();
        while self.toasts.front().map(|t| t.1.elapsed() > Duration::from_secs(6)).unwrap_or(false) {
            self.toasts.pop_front();
        }
    }

    // ------------------------------------------------------------ incoming frames

    fn on_frame(&mut self, f: &[u8]) {
        let Some(&code) = f.first() else { return };
        let Some(frame) = proto::parse(f) else {
            self.note(format!("Malformed frame code {code}: {}", hex(f)));
            return;
        };
        if Frame::is_push(code) {
            if let (Frame::Telemetry { prefix, data }, Some((Expect::SelfTelemetry, _))) = (&frame, &self.inflight) {
                if self.me.as_ref().map(|m| m.pubkey.starts_with(prefix)).unwrap_or(false) {
                    self.self_telemetry = proto::decode_lpp(data);
                    self.inflight = None;
                    return;
                }
            }
            self.on_push(frame);
            return;
        }
        // Contact streaming does not complete the in-flight command until END.
        if let (Frame::Contact(c), Some((Expect::Contacts, t))) = (&frame, &mut self.inflight) {
            *t = Instant::now();
            let c = c.clone();
            self.upsert_contact(c);
            return;
        }
        if let (Frame::ContactsStart, Some((Expect::Contacts, _))) = (&frame, &self.inflight) {
            return;
        }
        let expect = self.inflight.take().map(|e| e.0).unwrap_or(Expect::Quiet);
        self.on_reply(expect, frame);
    }

    fn on_reply(&mut self, expect: Expect, frame: Frame) {
        if let Frame::Err(code) = frame {
            self.on_error(expect, code);
            return;
        }
        if let Frame::Disabled = frame {
            self.toast("That feature is disabled in the radio firmware".into());
            return;
        }
        match (expect, frame) {
            (Expect::DeviceInfo, Frame::DeviceInfo(d)) => {
                self.note(format!("Device: {} {} ({}), protocol v{}", d.model, d.version, d.build_date, d.fw_ver));
                for i in 0..d.max_channels.max(1) {
                    self.push(proto::get_channel(i), Expect::Channel(i));
                }
                // Optional settings; older firmware answers "unsupported", which is ignored.
                self.push(proto::simple(proto::CMD_GET_AUTOADD_CONFIG), Expect::AutoAdd);
                self.push(proto::simple(proto::CMD_GET_TUNING_PARAMS), Expect::Tuning);
                self.push(proto::simple(proto::CMD_GET_DEFAULT_FLOOD_SCOPE), Expect::Scope);
                self.push(proto::simple(proto::CMD_GET_CUSTOM_VARS), Expect::CustomVars);
                if d.repeat.is_some() {
                    self.push(proto::simple(proto::CMD_GET_ALLOWED_REPEAT_FREQ), Expect::RepeatFreqs);
                }
                self.push(proto::simple(proto::CMD_GET_DEVICE_TIME), Expect::DeviceTime);
                // Messages are synced after channels are known so they can be attributed.
                self.syncing = true;
                self.push(proto::simple(proto::CMD_SYNC_NEXT_MESSAGE), Expect::Sync);
                self.device = Some(d);
            }
            (Expect::SelfInfo, Frame::SelfInfo(s)) => {
                if self.me.as_ref().map(|m| m.pubkey) != Some(s.pubkey) {
                    let (store, hist, next) = Store::open(&s.pubkey);
                    self.store = Some(store);
                    self.chats = hist.into_iter().map(|(k, msgs)| (k, Chat { msgs, unread: 0 })).collect();
                    self.next_id = next;
                    self.selected = None;
                }
                self.me = Some(s);
            }
            (Expect::Reload(label), Frame::Ok) => {
                self.toast(format!("{label}: saved"));
                self.push(proto::app_start(), Expect::SelfInfo);
            }
            (Expect::Label(l), Frame::Ok) => self.toast(format!("{l}: done")),
            (Expect::Contacts, Frame::EndOfContacts) => {
                self.note(format!("{} contacts loaded", self.contacts.len()));
            }
            (Expect::OneContact, Frame::Contact(c)) => self.upsert_contact(c),
            (Expect::Channel(i), Frame::ChannelInfo { idx, name, secret }) => {
                let _ = i;
                self.channels.retain(|c| c.idx != idx);
                if !name.is_empty() || secret != [0; 16] {
                    self.channels.push(Channel { idx, name, secret });
                    self.channels.sort_by_key(|c| c.idx);
                }
            }
            (Expect::Sync, Frame::Msg(m)) => {
                self.on_message(m);
                self.push(proto::simple(proto::CMD_SYNC_NEXT_MESSAGE), Expect::Sync);
            }
            (Expect::Sync, Frame::ChannelData) => {
                self.push(proto::simple(proto::CMD_SYNC_NEXT_MESSAGE), Expect::Sync);
            }
            (Expect::Sync, Frame::NoMoreMessages) => self.syncing = false,
            (Expect::Batt, Frame::Batt { mv, used_kb, total_kb }) => self.batt = Some((mv, used_kb, total_kb)),
            (Expect::Dm(key, id), Frame::Sent { flood, tag, timeout_ms }) => {
                let wait = Duration::from_millis((timeout_ms as u64).max(4000) * 3 / 2);
                let mut track = false;
                if let Some(m) = self.msg_mut(key, id) {
                    m.ack = tag;
                    m.status = Status::Sent;
                    if tag != 0 {
                        m.deadline = Some(Instant::now() + wait);
                        track = true;
                    }
                }
                if track && !self.awaiting_ack.contains(&(key, id)) {
                    self.awaiting_ack.push((key, id));
                }
                if flood {
                    self.note("Message sent by flood routing".into());
                }
            }
            (Expect::ChannelMsg(key, id), Frame::Ok) => self.set_status(key, id, Status::Sent),
            (Expect::Login(pk), Frame::Sent { .. }) => {
                self.remote(pk).busy = Some("Logging in…");
            }
            (Expect::Remote(pk, what), Frame::Sent { .. }) => {
                self.remote(pk).busy = Some(what);
            }
            (Expect::Trace(pk), Frame::Sent { tag, .. }) => {
                let ping = self.traces.get(&tag).map(|t| t.ping).unwrap_or(false);
                if let Some(t) = self.traces.get_mut(&tag) {
                    t.sent = Instant::now(); // round-trip time is measured from here
                }
                self.remote(pk).busy = Some(if ping { "Pinging…" } else { "Tracing…" });
            }
            (Expect::DeviceRefresh, Frame::DeviceInfo(d)) => self.device = Some(d),
            (Expect::AutoAdd, Frame::AutoAdd { config, max_hops }) => self.autoadd = Some((config, max_hops)),
            (Expect::Tuning, Frame::Tuning { rx_delay, airtime }) => self.tuning = Some((rx_delay, airtime)),
            (Expect::Scope, Frame::DefaultScope(sc)) => self.default_scope = Some(sc),
            (Expect::CustomVars, Frame::CustomVars(text)) => {
                self.custom_vars = text
                    .split(',')
                    .filter_map(|kv| kv.split_once(':'))
                    .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                    .filter(|(k, _)| !k.is_empty())
                    .collect();
            }
            (Expect::RepeatFreqs, Frame::RepeatFreqs(v)) => self.repeat_freqs = v,
            (Expect::DeviceTime, Frame::CurrTime(t)) => self.device_clock = Some((t, now())),
            (Expect::PrivateKey, Frame::PrivateKey(k)) => self.private_key = Some(k),
            (Expect::Cli, Frame::CliReply(t)) => {
                for line in t.lines() {
                    self.cli_log.push_back(line.to_string());
                }
                while self.cli_log.len() > 500 {
                    self.cli_log.pop_front();
                }
            }
            (Expect::AdvertPath(pk), Frame::AdvertPath { ts, path, hash_size }) => {
                let route = if path.is_empty() { "direct (no relays)".to_string() } else { self.describe_route(&path, hash_size) };
                self.remote(pk).advert_path = Some(format!("{route}, heard {}", util::ago(ts as u64)));
            }
            (Expect::Binary(pk, kind), Frame::Sent { tag, .. }) => {
                self.binary_pending.insert(tag, (pk, kind));
                self.remote(pk).busy = Some(if kind == BIN_NEIGHBOURS { "Requesting neighbours…" } else { "Requesting owner info…" });
            }
            (Expect::Export(label), Frame::ExportContact(b)) => {
                self.clipboard = Some(format!("meshcore://{}", hex(&b)));
                self.toast(format!("{label} copied to clipboard"));
            }
            (Expect::Stats(k), Frame::Stats(kind, d)) if (kind as usize) < 3 => {
                let _ = k;
                self.stats[kind as usize] = proto::decode_local_stats(kind, &d);
            }
            (Expect::Quiet, _) => {}
            (e, f) => self.note(format!("Unexpected reply {f:?} for {e:?}")),
        }
    }

    fn on_error(&mut self, expect: Expect, code: u8) {
        let why = proto::err_text(code);
        match expect {
            Expect::Quiet => {}
            Expect::Sync => self.syncing = false,
            Expect::Channel(_) => {}
            Expect::Dm(key, id) | Expect::ChannelMsg(key, id) => {
                self.set_status(key, id, Status::Failed);
                self.toast(format!("Send failed: {why}"));
            }
            // Optional settings and look-ups: older firmware simply doesn't support them.
            Expect::AutoAdd | Expect::Tuning | Expect::Scope | Expect::CustomVars | Expect::RepeatFreqs | Expect::DeviceTime | Expect::OneContact | Expect::DeviceRefresh => {}
            Expect::AdvertPath(pk) => self.remote(pk).advert_path = None,
            Expect::Cli => self.toast(format!("Command failed: {why} (the local console needs firmware v1.18 or later)")),
            Expect::Login(pk) | Expect::Remote(pk, _) | Expect::Trace(pk) | Expect::Binary(pk, _) => {
                self.remote(pk).busy = None;
                self.toast(format!("Request failed: {why}"));
            }
            e => {
                let label = match e {
                    Expect::Label(l) | Expect::Reload(l) | Expect::Export(l) => l,
                    e => format!("{e:?}"),
                };
                self.toast(format!("{label}: {why}"));
            }
        }
    }

    fn on_push(&mut self, f: Frame) {
        match f {
            Frame::MsgWaiting => {
                if !self.syncing {
                    self.syncing = true;
                    self.push(proto::simple(proto::CMD_SYNC_NEXT_MESSAGE), Expect::Sync);
                }
            }
            Frame::SendConfirmed { ack, rtt_ms } => {
                if let Some(pos) = self.awaiting_ack.iter().position(|(k, id)| self.msg_ref(*k, *id).map(|m| m.ack == ack).unwrap_or(false)) {
                    let (k, id) = self.awaiting_ack.remove(pos);
                    self.set_status(k, id, Status::Delivered(rtt_ms));
                }
            }
            // Heard an advert from a node the radio did NOT add (manual mode, hop limit or full list).
            Frame::NewAdvert(c) => {
                if self.contact(&c.pubkey).is_none() {
                    self.note(format!("Discovered {} ({}), not added", c.name, proto::type_name(c.kind)));
                    self.discovered.retain(|d| d.pubkey != c.pubkey);
                    self.discovered.insert(0, c);
                    self.discovered.truncate(100);
                }
            }
            // A contact was added or updated on the radio: fetch its current details.
            Frame::Advert(pk) | Frame::PathUpdated(pk) => {
                self.discovered.retain(|d| d.pubkey != pk);
                self.push(proto::with_key(proto::CMD_GET_CONTACT_BY_KEY, &pk), Expect::OneContact);
            }
            Frame::BinaryResponse { tag, data } => {
                if let Some((pk, kind)) = self.binary_pending.remove(&tag) {
                    let r = self.remote(pk);
                    r.busy = None;
                    if kind == BIN_NEIGHBOURS {
                        match proto::decode_neighbours(&data, 4) {
                            Some((total, list)) => r.neighbours = Some((total, list, now())),
                            None => self.toast("Could not decode the neighbour list".into()),
                        }
                    } else {
                        r.owner = Some(String::from_utf8_lossy(&data).trim_end_matches('\0').to_string());
                    }
                }
            }
            Frame::ContactDeleted(pk) => self.contacts.retain(|c| c.pubkey != pk),
            Frame::ContactsFull => self.toast("The radio's contact list is full".into()),
            Frame::LoginSuccess { admin, prefix } => {
                if let Some(pk) = self.contact_by_prefix(&prefix).map(|c| c.pubkey) {
                    let r = self.remote(pk);
                    r.logged_in = Some(admin);
                    r.busy = None;
                    let name = self.name_of(&pk);
                    self.toast(format!("Logged in to {name}{}", if admin { " (admin)" } else { "" }));
                }
            }
            Frame::LoginFail { prefix } => {
                if let Some(pk) = self.contact_by_prefix(&prefix).map(|c| c.pubkey) {
                    let r = self.remote(pk);
                    r.logged_in = None;
                    r.busy = None;
                    self.toast("Login failed – wrong password?".into());
                }
            }
            Frame::StatusResponse { prefix, data } => {
                if let Some(pk) = self.contact_by_prefix(&prefix).map(|c| c.pubkey) {
                    let r = self.remote(pk);
                    r.status = proto::decode_status(&data);
                    r.status_at = now();
                    r.busy = None;
                }
            }
            Frame::Telemetry { prefix, data } => {
                if let Some(pk) = self.contact_by_prefix(&prefix).map(|c| c.pubkey) {
                    let r = self.remote(pk);
                    r.telemetry = proto::decode_lpp(&data);
                    r.telemetry_at = now();
                    r.busy = None;
                }
            }
            Frame::PathDiscovery { prefix, out_path, in_path } => {
                if let Some(pk) = self.contact_by_prefix(&prefix).map(|c| c.pubkey) {
                    let fmt = |p: &[u8]| if p.is_empty() { "direct".to_string() } else { p.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" → ") };
                    let r = self.remote(pk);
                    r.discovery = Some(format!("Outbound: {}\nReturn: {}", fmt(&out_path), fmt(&in_path)));
                    r.busy = None;
                    self.push(proto::with_key(30, &pk), Expect::OneContact);
                }
            }
            Frame::Trace { tag, flags, hashes, snrs, final_snr } => {
                if let Some(t) = self.traces.remove(&tag) {
                    let pk = t.pk;
                    if t.ping {
                        let rtt = t.sent.elapsed().as_millis();
                        let sz = 1usize << (flags & 3);
                        // The repeater's own hop: first occurrence of its hash in the trace path.
                        let idx = hashes.chunks(sz).position(|h| pk.starts_with(h)).unwrap_or(0);
                        let there = snrs.get(idx).map(|v| format!("{:+.1} dB", *v as f32 / 4.0)).unwrap_or_else(|| "?".into());
                        let s = format!("{rtt} ms round trip · heard there at {there} · reply heard here at {final_snr:+.1} dB");
                        self.note(format!("Ping {}: {s}", self.name_of(&pk)));
                        let r = self.remote(pk);
                        r.ping = Some(format!("{s} ({})", util::fmt_clock(now())));
                        r.busy = None;
                        return;
                    }
                    let sz = 1usize << (flags & 3);
                    let mut s = String::from("me");
                    for (i, h) in hashes.chunks(sz).enumerate() {
                        let snr = snrs.get(i).map(|v| *v as f32 / 4.0).unwrap_or(0.0);
                        s += &format!(" → {} ({snr:+.1} dB)", hex(h));
                    }
                    s += &format!(" → me ({final_snr:+.1} dB)");
                    self.note(format!("Trace to {}: {s}", self.name_of(&pk)));
                    let r = self.remote(pk);
                    r.trace = Some(s);
                    r.busy = None;
                }
            }
            Frame::LogRx { snr, rssi, raw } => {
                self.match_repeat(&raw, snr);
                let text = format!("SNR {snr:+.1} dB  RSSI {rssi} dBm  {}  {}", describe_packet(&raw), hex(&raw));
                if self.rx_log.len() >= 500 {
                    self.rx_log.pop_front();
                }
                self.rx_log.push_back(LogLine { t: now(), text });
            }
            Frame::Other(c) => self.note(format!("Unhandled push 0x{c:02x}")),
            _ => {}
        }
    }

    fn on_message(&mut self, m: proto::RxMsg) {
        let (key, author) = if let Some(idx) = m.channel {
            let Some(ch) = self.channels.iter().find(|c| c.idx == idx) else {
                self.note(format!("Message on unknown channel {idx}: {}", m.text));
                return;
            };
            (ChatKey::Channel(ch.secret), String::new())
        } else {
            let pk = match self.contact_by_prefix(&m.prefix) {
                Some(c) => c.pubkey,
                None => {
                    let mut pk = [0u8; 32];
                    pk[..6].copy_from_slice(&m.prefix);
                    pk
                }
            };
            let author = match m.author {
                Some(a) if self.me.as_ref().map(|me| me.pubkey[..4] == a).unwrap_or(false) => "me".to_string(),
                Some(a) => self.contacts.iter().find(|c| c.pubkey[..4] == a).map(|c| c.name.clone()).unwrap_or_else(|| hex(&a)),
                None => String::new(),
            };
            (ChatKey::Contact(pk), author)
        };
        let (author, text) = if m.channel.is_some() {
            match m.text.split_once(": ") {
                Some((a, t)) if a.len() <= 32 => (a.to_string(), t.to_string()),
                _ => (String::new(), m.text.clone()),
            }
        } else {
            (author, m.text.clone())
        };
        if let ChatKey::Contact(pk) = key {
            if self.blocked_keys.contains(&pk) {
                self.note(format!("Dropped message from blocked contact {}", self.name_of(&pk)));
                return;
            }
        }
        if !author.is_empty() && self.blocked_names.contains(&author) {
            self.note(format!("Dropped message from blocked sender {author}"));
            return;
        }
        // Duplicate suppression (the same message can arrive via several paths).
        if let Some(chat) = self.chats.get(&key) {
            if chat.msgs.iter().rev().take(50).any(|x| !x.out && x.ts == m.timestamp && x.text == text && x.author == author) {
                return;
            }
        }
        let msg = Message {
            id: self.next_id,
            out: false,
            ts: m.timestamp,
            recv: now(),
            author,
            text,
            status: Status::Received,
            txt_type: m.txt_type,
            snr: m.snr,
            path_len: m.path_len,
            ack: 0,
            attempt: 0,
            deadline: None,
            repeats: Vec::new(),
        };
        self.next_id += 1;
        if let Some(s) = &mut self.store {
            s.add(&key, &msg);
        }
        let chat = self.chats.entry(key).or_default();
        chat.msgs.push(msg);
        if self.selected != Some(key) {
            chat.unread += 1;
        }
        self.attention = true;
    }

    // ------------------------------------------------------------ delivery tracking

    fn check_acks(&mut self) {
        let nowi = Instant::now();
        let mut i = 0;
        while i < self.awaiting_ack.len() {
            let (key, id) = self.awaiting_ack[i];
            let Some(m) = self.msg_ref(key, id) else {
                self.awaiting_ack.remove(i);
                continue;
            };
            if m.status != Status::Sent || m.deadline.map(|d| d > nowi).unwrap_or(true) {
                if m.status != Status::Sent {
                    self.awaiting_ack.remove(i);
                } else {
                    i += 1;
                }
                continue;
            }
            let (attempt, ts, text) = (m.attempt + 1, m.ts, m.text.clone());
            self.awaiting_ack.remove(i);
            let ChatKey::Contact(pk) = key else { continue };
            if attempt >= MAX_ATTEMPTS || !self.connected() {
                self.set_status(key, id, Status::Failed);
                continue;
            }
            if let Some(m) = self.msg_mut(key, id) {
                m.attempt = attempt;
                m.status = Status::Pending;
                m.deadline = None;
            }
            if attempt == MAX_ATTEMPTS - 1 {
                // Final attempt: forget the (apparently broken) route so the radio floods.
                self.push(proto::with_key(proto::CMD_RESET_PATH, &pk), Expect::Quiet);
            }
            self.push(proto::send_txt(proto::TXT_PLAIN, attempt, ts, &pk, &text), Expect::Dm(key, id));
        }
    }

    // ------------------------------------------------------------ user actions

    pub fn send_text(&mut self, key: ChatKey, text: &str) {
        let text = text.trim();
        if text.is_empty() || !self.connected() {
            return;
        }
        let ts = now() as u32;
        let (frame, txt_type) = match key {
            ChatKey::Channel(secret) => {
                let Some(ch) = self.channels.iter().find(|c| c.secret == secret) else { return };
                (proto::send_channel_txt(ch.idx, ts, text), proto::TXT_PLAIN)
            }
            ChatKey::Contact(pk) => {
                let t = if self.contact(&pk).map(|c| c.kind == proto::ADV_REPEATER).unwrap_or(false) { proto::TXT_CLI_DATA } else { proto::TXT_PLAIN };
                (proto::send_txt(t, 0, ts, &pk, text), t)
            }
        };
        let author = self.me.as_ref().map(|m| m.name.clone()).unwrap_or_default();
        let msg = Message {
            id: self.next_id,
            out: true,
            ts,
            recv: now(),
            author,
            text: text.to_string(),
            status: Status::Pending,
            txt_type,
            snr: None,
            path_len: 0xFF,
            ack: 0,
            attempt: 0,
            deadline: None,
            repeats: Vec::new(),
        };
        let id = msg.id;
        self.next_id += 1;
        if let Some(s) = &mut self.store {
            s.add(&key, &msg);
        }
        self.chats.entry(key).or_default().msgs.push(msg);
        let expect = if matches!(key, ChatKey::Channel(_)) { Expect::ChannelMsg(key, id) } else { Expect::Dm(key, id) };
        self.push(frame, expect);
    }

    pub fn advert(&mut self, flood: bool) {
        let l = if flood { "Flood advert" } else { "Zero-hop advert" };
        self.push(proto::self_advert(flood), Expect::Label(l.into()));
    }

    pub fn reset_path(&mut self, pk: [u8; 32]) {
        self.push(proto::with_key(proto::CMD_RESET_PATH, &pk), Expect::Label("Path reset".into()));
        self.push(proto::with_key(30, &pk), Expect::OneContact);
    }

    pub fn share_contact(&mut self, pk: [u8; 32]) {
        self.push(proto::with_key(proto::CMD_SHARE_CONTACT, &pk), Expect::Label("Contact shared (zero-hop)".into()));
    }

    pub fn export_contact(&mut self, pk: Option<[u8; 32]>) {
        match pk {
            Some(pk) => self.push(proto::with_key(proto::CMD_EXPORT_CONTACT, &pk), Expect::Export("Contact link".into())),
            None => self.push(proto::export_self(), Expect::Export("Your contact link".into())),
        }
    }

    pub fn remove_contact(&mut self, pk: [u8; 32]) {
        self.push(proto::with_key(proto::CMD_REMOVE_CONTACT, &pk), Expect::Label("Contact removed".into()));
        self.contacts.retain(|c| c.pubkey != pk);
        if self.selected == Some(ChatKey::Contact(pk)) {
            self.selected = None;
        }
    }

    pub fn toggle_favourite(&mut self, pk: [u8; 32]) {
        if let Some(c) = self.contacts.iter_mut().find(|c| c.pubkey == pk) {
            c.flags ^= 1;
            let f = proto::update_contact(c);
            self.push(f, Expect::Quiet);
        }
    }

    /// Accepts `meshcore://<hex advert>` or `meshcore://contact/add?name=..&public_key=..&type=..`.
    pub fn import_contact(&mut self, s: &str) -> Result<(), String> {
        let s = s.trim();
        let body = s.strip_prefix("meshcore://").unwrap_or(s);
        if let Some(q) = body.strip_prefix("contact/add?") {
            let get = |k: &str| q.split('&').find_map(|kv| kv.strip_prefix(k).and_then(|v| v.strip_prefix('='))).map(url_decode);
            let pk: [u8; 32] = get("public_key").and_then(|h| util::unhex(&h)).and_then(|v| v.try_into().ok()).ok_or("missing or bad public_key")?;
            let c = Contact {
                pubkey: pk,
                kind: get("type").and_then(|t| t.parse().ok()).unwrap_or(proto::ADV_CHAT),
                flags: 0,
                out_path_len: proto::OUT_PATH_UNKNOWN,
                out_path: [0; 64],
                name: get("name").unwrap_or_else(|| hex(&pk[..4])),
                last_advert: 0,
                lat: 0.0,
                lon: 0.0,
                lastmod: 0,
            };
            self.push(proto::update_contact(&c), Expect::Label("Contact added".into()));
            self.push(proto::with_key(30, &pk), Expect::OneContact);
            return Ok(());
        }
        let bytes = util::unhex(body).ok_or("not a meshcore:// contact link")?;
        if bytes.len() < 100 {
            return Err("contact data too short".into());
        }
        self.push(proto::import_contact(&bytes), Expect::Label("Contact imported".into()));
        self.push(proto::get_contacts(0), Expect::Contacts);
        Ok(())
    }

    pub fn login(&mut self, pk: [u8; 32], password: &str) {
        self.remote(pk).busy = Some("Logging in…");
        self.push(proto::login(&pk, password), Expect::Login(pk));
    }

    pub fn logout(&mut self, pk: [u8; 32]) {
        self.remote(pk).logged_in = None;
        self.push(proto::with_key(proto::CMD_LOGOUT, &pk), Expect::Quiet);
    }

    pub fn request_status(&mut self, pk: [u8; 32]) {
        self.push(proto::with_key(proto::CMD_SEND_STATUS_REQ, &pk), Expect::Remote(pk, "Requesting status…"));
    }

    pub fn request_telemetry(&mut self, pk: [u8; 32]) {
        self.push(proto::telemetry_req(&pk), Expect::Remote(pk, "Requesting telemetry…"));
    }

    pub fn path_discovery(&mut self, pk: [u8; 32]) {
        self.push(proto::path_discovery(&pk), Expect::Remote(pk, "Discovering path…"));
    }

    /// Trace the known route to a repeater and back again.
    pub fn trace(&mut self, pk: [u8; 32]) -> Result<(), String> {
        self.send_trace(pk, false)
    }

    /// Ping a repeater: a trace out and back, timed. Uses the known route, or assumes the
    /// repeater is a direct neighbour if no route is known.
    pub fn ping(&mut self, pk: [u8; 32]) -> Result<(), String> {
        self.send_trace(pk, true)
    }

    fn send_trace(&mut self, pk: [u8; 32], ping: bool) -> Result<(), String> {
        let c = self.contact(&pk).ok_or("unknown contact")?;
        let (hops, mut sz): (Vec<Vec<u8>>, usize) = if c.out_path_len == proto::OUT_PATH_UNKNOWN {
            if !ping {
                return Err("no known route – run path discovery, or use Ping if the repeater is in direct range".into());
            }
            (Vec::new(), 1)
        } else {
            let (_, sz) = proto::path_dims(c.out_path_len);
            (c.path_bytes().chunks(sz).map(|h| h.to_vec()).collect(), sz)
        };
        if sz == 3 {
            sz = 1; // not representable in trace flags; fall back to 1-byte hashes
        }
        let mut path = Vec::new();
        for h in &hops {
            path.extend_from_slice(&h[..sz]);
        }
        path.extend_from_slice(&c.pubkey[..sz]);
        for h in hops.iter().rev() {
            path.extend_from_slice(&h[..sz]);
        }
        let flags = match sz {
            2 => 1,
            4 => 2,
            _ => 0,
        };
        let tag = util::rand_u32();
        self.traces.insert(tag, TraceReq { pk, ping, sent: Instant::now() });
        let r = self.remote(pk);
        if ping {
            r.ping = None;
        } else {
            r.trace = None;
        }
        self.push(proto::trace(tag, flags, &path), Expect::Trace(pk));
        Ok(())
    }

    pub fn request_neighbours(&mut self, pk: [u8; 32]) {
        self.push(proto::neighbours_req(&pk, 20), Expect::Binary(pk, BIN_NEIGHBOURS));
    }

    pub fn request_owner(&mut self, pk: [u8; 32]) {
        self.push(proto::owner_req(&pk), Expect::Binary(pk, BIN_OWNER));
    }

    pub fn request_advert_path(&mut self, pk: [u8; 32]) {
        self.push(proto::get_advert_path(&pk), Expect::AdvertPath(pk));
    }

    /// Set a contact's route manually (hash bytes, `sz` bytes per hop), or `None` for flood.
    pub fn set_contact_path(&mut self, pk: [u8; 32], path: Option<(Vec<u8>, usize)>) {
        let Some(c) = self.contact(&pk).cloned() else { return };
        let f = proto::set_contact_path(&c, path.as_ref().map(|(p, sz)| (p.as_slice(), *sz)));
        self.push(f, Expect::Label("Route updated".into()));
        self.push(proto::with_key(proto::CMD_GET_CONTACT_BY_KEY, &pk), Expect::OneContact);
    }

    /// Add a node from the "discovered" list to the radio's contacts.
    pub fn add_discovered(&mut self, pk: [u8; 32]) {
        if let Some(pos) = self.discovered.iter().position(|d| d.pubkey == pk) {
            let c = self.discovered.remove(pos);
            self.push(proto::update_contact(&c), Expect::Label(format!("{} added to contacts", c.name)));
            self.push(proto::with_key(proto::CMD_GET_CONTACT_BY_KEY, &pk), Expect::OneContact);
        }
    }

    pub fn dismiss_discovered(&mut self, pk: [u8; 32]) {
        self.discovered.retain(|d| d.pubkey != pk);
    }

    /// Names (or hex) for a list of relay hashes.
    pub fn describe_route(&self, path: &[u8], sz: usize) -> String {
        path.chunks(sz.max(1)).map(|h| self.relay_name(h)).collect::<Vec<_>>().join(" → ")
    }

    /// Best guess at which node a path hash refers to (repeaters preferred).
    pub fn relay_name(&self, hash: &[u8]) -> String {
        if hash.is_empty() {
            return "unknown relay".into();
        }
        let mut m: Vec<&Contact> = self.contacts.iter().filter(|c| c.pubkey.starts_with(hash)).collect();
        m.sort_by_key(|c| (c.kind != proto::ADV_REPEATER, std::cmp::Reverse(c.last_advert)));
        match m.first() {
            Some(c) if m.len() == 1 => format!("{} ({})", c.name, hex(hash)),
            Some(c) => format!("{}? ({})", c.name, hex(hash)),
            None => hex(hash),
        }
    }

    /// Recognise our own outgoing messages being re-transmitted by relays.
    fn match_repeat(&mut self, raw: &[u8], snr: f32) {
        let Some(p) = proto::parse_raw(raw) else { return };
        let flood = matches!(raw[0] & 3, 0 | 1);
        let Some(me) = self.me.as_ref() else { return };
        let my_name = format!("{}: ", me.name);
        let my_hash = me.pubkey[0];
        match p.payload_type {
            proto::PAYLOAD_GRP_TXT if flood && !p.path.is_empty() => {
                let relay = p.path[p.path.len() - p.hash_size..].to_vec();
                let found = self.channels.iter().find_map(|ch| proto::decrypt_group_text(p.payload, &ch.secret).map(|r| (ch.secret, r)));
                if let Some((secret, (ts, text))) = found {
                    let Some(body) = text.strip_prefix(&my_name) else { return };
                    let key = ChatKey::Channel(secret);
                    let id = self.chats.get(&key).and_then(|c| c.msgs.iter().rev().find(|m| m.out && m.ts == ts && m.text == body).map(|m| m.id));
                    if let Some(id) = id {
                        self.add_repeat(key, id, relay, snr);
                    }
                }
            }
            proto::PAYLOAD_TXT_MSG if p.payload.len() >= 2 && p.payload[1] == my_hash => {
                // Direct message from us (src hash) to a contact (dest hash): attribute it to our latest
                // outgoing message to that contact sent within the last minute.
                let dest = p.payload[0];
                let t = now();
                let target = self
                    .chats
                    .iter()
                    .filter(|(k, _)| matches!(k, ChatKey::Contact(pk) if pk[0] == dest))
                    .filter_map(|(k, c)| c.msgs.iter().rev().find(|m| m.out && m.txt_type == proto::TXT_PLAIN && t.saturating_sub(m.recv) < 60).map(|m| (*k, m.id, m.recv)))
                    .max_by_key(|x| x.2);
                let Some((key, id, _)) = target else { return };
                let relay = if flood {
                    if p.path.is_empty() {
                        return;
                    }
                    p.path[p.path.len() - p.hash_size..].to_vec()
                } else {
                    // Direct routing: each relay removes itself, so the relay we heard is the hop
                    // just before the remaining path in the contact's route.
                    let ChatKey::Contact(pk) = key else { return };
                    match self.contact(&pk) {
                        Some(c) if c.out_path_len != proto::OUT_PATH_UNKNOWN => {
                            let (n, sz) = proto::path_dims(c.out_path_len);
                            let remaining = p.path.len() / p.hash_size.max(1);
                            match n.checked_sub(remaining + 1) {
                                Some(i) => c.path_bytes()[i * sz..(i + 1) * sz].to_vec(),
                                None => return,
                            }
                        }
                        _ => Vec::new(),
                    }
                };
                self.add_repeat(key, id, relay, snr);
            }
            _ => {}
        }
    }

    fn add_repeat(&mut self, key: ChatKey, id: u64, relay: Vec<u8>, snr: f32) {
        let Some(m) = self.msg_mut(key, id) else { return };
        if !relay.is_empty() && m.repeats.iter().any(|(h, _)| *h == relay) {
            return;
        }
        m.repeats.push((relay.clone(), snr));
        if let Some(st) = &mut self.store {
            st.add_repeat(id, &relay, snr);
        }
    }

    // ------------------------------------------------------------ device settings

    pub fn save_autoadd(&mut self, config: u8, max_hops: u8) {
        self.push(proto::set_autoadd(config, max_hops), Expect::Label("Contact auto-add settings saved".into()));
        self.push(proto::simple(proto::CMD_GET_AUTOADD_CONFIG), Expect::AutoAdd);
    }
    pub fn save_tuning(&mut self, rx_delay_x1000: u32, airtime_x1000: u32) {
        self.push(proto::set_tuning(rx_delay_x1000, airtime_x1000), Expect::Label("Tuning saved".into()));
        self.push(proto::simple(proto::CMD_GET_TUNING_PARAMS), Expect::Tuning);
    }
    pub fn save_scope(&mut self, name: Option<&str>) {
        let f = match name {
            Some(n) => {
                let n = n.trim().trim_start_matches('#');
                proto::set_default_scope(Some((n, &util::hashtag_secret(&format!("#{n}")))))
            }
            None => proto::set_default_scope(None),
        };
        self.push(f, Expect::Label("Region saved".into()));
        self.push(proto::simple(proto::CMD_GET_DEFAULT_FLOOD_SCOPE), Expect::Scope);
    }
    pub fn save_custom_var(&mut self, name: &str, value: &str) {
        self.push(proto::set_custom_var(name, value), Expect::Label(format!("{name} saved")));
        self.push(proto::simple(proto::CMD_GET_CUSTOM_VARS), Expect::CustomVars);
    }
    pub fn save_path_hash_mode(&mut self, mode: u8) {
        self.push(proto::set_path_hash_mode(mode), Expect::Label("Path hash size saved".into()));
        self.push(proto::device_query(), Expect::DeviceRefresh);
    }
    pub fn read_device_clock(&mut self) {
        self.push(proto::simple(proto::CMD_GET_DEVICE_TIME), Expect::DeviceTime);
    }
    pub fn request_self_telemetry(&mut self) {
        self.push(proto::self_telemetry_req(), Expect::SelfTelemetry);
    }
    pub fn export_private_key(&mut self) {
        self.push(proto::simple(proto::CMD_EXPORT_PRIVATE_KEY), Expect::PrivateKey);
    }
    pub fn import_private_key(&mut self, key: [u8; 64]) {
        self.push(proto::import_private_key(&key), Expect::Reload("Private key".into()));
        self.push(proto::get_contacts(0), Expect::Contacts);
    }
    pub fn factory_reset(&mut self) {
        if let Some(l) = &self.link {
            l.send(proto::factory_reset());
        }
        self.toast("Factory reset requested – the radio will erase its settings and restart".into());
    }
    pub fn run_cli(&mut self, cmd: &str) {
        self.cli_log.push_back(format!("> {cmd}"));
        self.push(proto::run_cli(cmd), Expect::Cli);
    }

    pub fn set_channel(&mut self, idx: u8, name: &str, secret: [u8; 16]) {
        self.push(proto::set_channel(idx, name, &secret), Expect::Label(if name.is_empty() { "Channel removed".into() } else { format!("Channel '{name}' saved") }));
        self.push(proto::get_channel(idx), Expect::Channel(idx));
    }

    pub fn free_channel_slot(&self) -> Option<u8> {
        let max = self.device.as_ref().map(|d| d.max_channels).unwrap_or(8);
        (0..max).find(|i| !self.channels.iter().any(|c| c.idx == *i))
    }

    pub fn save_name(&mut self, name: &str) {
        self.push(proto::set_name(name), Expect::Reload("Name".into()));
    }
    pub fn save_location(&mut self, lat: f64, lon: f64) {
        self.push(proto::set_latlon(lat, lon), Expect::Reload("Location".into()));
    }
    pub fn save_radio(&mut self, freq_khz: u32, bw_hz: u32, sf: u8, cr: u8, repeat: Option<bool>) {
        let f = match repeat {
            Some(rep) => proto::set_radio_repeat(freq_khz, bw_hz, sf, cr, rep),
            None => proto::set_radio(freq_khz, bw_hz, sf, cr),
        };
        self.push(f, Expect::Reload("Radio parameters".into()));
        if repeat.is_some() {
            self.push(proto::device_query(), Expect::DeviceRefresh);
        }
    }
    pub fn save_tx_power(&mut self, dbm: i8) {
        self.push(proto::set_tx_power(dbm), Expect::Reload("Transmit power".into()));
    }
    pub fn save_other(&mut self, manual_add: u8, telemetry: u8, loc_policy: u8, multi_acks: u8) {
        self.push(proto::set_other(manual_add, telemetry, loc_policy, multi_acks), Expect::Reload("Preferences".into()));
    }
    pub fn save_pin(&mut self, pin: u32) {
        self.push(proto::set_pin(pin), Expect::Label("BLE PIN (takes effect after reboot)".into()));
    }
    pub fn reboot(&mut self) {
        if let Some(l) = &self.link {
            l.send(proto::reboot());
        }
        self.toast("Reboot requested".into());
    }
    pub fn sync_clock(&mut self) {
        self.push(proto::set_time(now() as u32), Expect::Label("Clock sync".into()));
    }
    pub fn refresh_stats(&mut self) {
        for k in 0..3 {
            self.push(proto::get_stats(k), Expect::Stats(k));
        }
        self.push(proto::simple(proto::CMD_GET_BATT_AND_STORAGE), Expect::Batt);
    }
    pub fn refresh_contacts(&mut self) {
        self.push(proto::get_contacts(0), Expect::Contacts);
    }

    pub fn clear_history(&mut self, key: ChatKey) {
        if let Some(c) = self.chats.get_mut(&key) {
            c.msgs.clear();
            c.unread = 0;
        }
        self.awaiting_ack.retain(|(k, _)| *k != key);
        if let Some(s) = &mut self.store {
            s.clear(&key);
        }
    }

    pub fn delete_message(&mut self, key: ChatKey, id: u64) {
        if let Some(c) = self.chats.get_mut(&key) {
            c.msgs.retain(|m| m.id != id);
        }
        self.awaiting_ack.retain(|e| *e != (key, id));
        if let Some(s) = &mut self.store {
            s.delete(id);
        }
    }

    pub fn find_message(&self, key: ChatKey, id: u64) -> Option<&Message> {
        self.msg_ref(key, id)
    }

    pub fn mark_unread(&mut self, key: ChatKey) {
        if self.selected == Some(key) {
            self.selected = None;
        }
        if let Some(c) = self.chats.get_mut(&key) {
            c.unread = c.unread.max(1);
        }
    }

    // ------------------------------------------------------------ blocking

    fn save_blocks(&mut self) {
        let keys: Vec<String> = self.blocked_keys.iter().map(|k| hex(k)).collect();
        self.settings.set("blocked_keys", &keys.join(","));
        let names = self.blocked_names.join("\n");
        self.settings.set("blocked_names", &names);
    }

    pub fn is_key_blocked(&self, pk: &[u8; 32]) -> bool {
        self.blocked_keys.contains(pk)
    }

    pub fn is_name_blocked(&self, name: &str) -> bool {
        !name.is_empty() && self.blocked_names.iter().any(|n| n == name)
    }

    pub fn set_key_blocked(&mut self, pk: [u8; 32], block: bool) {
        self.blocked_keys.retain(|k| *k != pk);
        if block {
            self.blocked_keys.push(pk);
        }
        self.save_blocks();
        let name = self.name_of(&pk);
        self.toast(format!("{name} {}", if block { "blocked" } else { "unblocked" }));
    }

    pub fn set_name_blocked(&mut self, name: &str, block: bool) {
        self.blocked_names.retain(|n| n != name);
        if block {
            self.blocked_names.push(name.to_string());
        }
        self.save_blocks();
        self.toast(format!("Sender '{name}' {}", if block { "blocked" } else { "unblocked" }));
    }

    /// Find a contact by its advertised name (for channel/room authors).
    pub fn contact_by_name(&self, name: &str) -> Option<&Contact> {
        if name.is_empty() {
            return None;
        }
        self.contacts.iter().find(|c| c.name == name)
    }

    // ------------------------------------------------------------ helpers

    pub fn note(&mut self, text: String) {
        if self.log.len() >= 300 {
            self.log.pop_front();
        }
        self.log.push_back(LogLine { t: now(), text });
    }

    pub fn toast(&mut self, text: String) {
        self.note(text.clone());
        self.toasts.push_back((text, Instant::now()));
        if self.toasts.len() > 4 {
            self.toasts.pop_front();
        }
    }

    pub fn contact(&self, pk: &[u8; 32]) -> Option<&Contact> {
        self.contacts.iter().find(|c| &c.pubkey == pk)
    }

    pub fn contact_by_prefix(&self, p: &[u8]) -> Option<&Contact> {
        self.contacts.iter().find(|c| c.pubkey.starts_with(p))
    }

    pub fn name_of(&self, pk: &[u8; 32]) -> String {
        self.contact(pk).map(|c| c.name.clone()).unwrap_or_else(|| format!("Unknown {}", hex(&pk[..6])))
    }

    pub fn chat_title(&self, key: &ChatKey) -> String {
        match key {
            ChatKey::Channel(s) => self.channels.iter().find(|c| &c.secret == s).map(|c| c.name.clone()).unwrap_or_else(|| "Old channel".into()),
            ChatKey::Contact(pk) => self.name_of(pk),
        }
    }

    pub fn remote(&mut self, pk: [u8; 32]) -> &mut Remote {
        self.remotes.entry(pk).or_default()
    }

    fn upsert_contact(&mut self, c: Contact) {
        match self.contacts.iter_mut().find(|x| x.pubkey == c.pubkey) {
            Some(x) => *x = c,
            None => self.contacts.push(c),
        }
    }

    fn msg_ref(&self, key: ChatKey, id: u64) -> Option<&Message> {
        self.chats.get(&key)?.msgs.iter().rev().find(|m| m.id == id)
    }

    fn msg_mut(&mut self, key: ChatKey, id: u64) -> Option<&mut Message> {
        self.chats.get_mut(&key)?.msgs.iter_mut().rev().find(|m| m.id == id)
    }

    fn set_status(&mut self, key: ChatKey, id: u64, s: Status) {
        if let Some(m) = self.msg_mut(key, id) {
            m.status = s;
            m.deadline = None;
            if let Some(st) = &mut self.store {
                st.set_status(id, s);
            }
        }
    }

    pub fn total_unread(&self) -> u32 {
        self.chats.values().map(|c| c.unread).sum()
    }
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let hexv = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => match (b.get(i + 1).and_then(|&c| hexv(c)), b.get(i + 2).and_then(|&c| hexv(c))) {
                (Some(h), Some(l)) => {
                    out.push(h << 4 | l);
                    i += 3;
                    continue;
                }
                _ => out.push(b'%'),
            },
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|c| if c.is_ascii_alphanumeric() || b"-_.~".contains(&c) { (c as char).to_string() } else { format!("%{c:02X}") })
        .collect()
}

/// One-line summary of a raw over-the-air packet header.
pub fn describe_packet(raw: &[u8]) -> String {
    let Some(&h) = raw.first() else { return String::new() };
    let route = ["T-flood", "flood", "direct", "T-direct"][(h & 3) as usize];
    let kind = match (h >> 2) & 15 {
        0 => "REQ",
        1 => "RESPONSE",
        2 => "TXT",
        3 => "ACK",
        4 => "ADVERT",
        5 => "GRP_TXT",
        6 => "GRP_DATA",
        7 => "ANON_REQ",
        8 => "PATH",
        9 => "TRACE",
        10 => "MULTIPART",
        11 => "CONTROL",
        15 => "RAW",
        _ => "?",
    };
    let off = if h & 3 == 0 || h & 3 == 3 { 5 } else { 1 };
    let hops = raw.get(off).map(|&p| p & 63).unwrap_or(0);
    format!("{kind:<9} {route:<8} {hops} hop(s)")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        unsafe { std::env::set_var("XDG_DATA_HOME", std::env::temp_dir().join("mc-app-test")) };
        let mut app = App::new(Arc::new(|| {}));
        app.me = Some(SelfInfo { name: "Desk Node".into(), pubkey: [0xD1; 32], ..Default::default() });
        app
    }

    fn outgoing(app: &mut App, key: ChatKey, ts: u32, text: &str) -> u64 {
        let id = app.next_id;
        app.next_id += 1;
        let m = Message {
            id,
            out: true,
            ts,
            recv: now(),
            author: "Desk Node".into(),
            text: text.into(),
            status: Status::Sent,
            txt_type: proto::TXT_PLAIN,
            snr: None,
            path_len: 0xFF,
            ack: 0,
            attempt: 0,
            deadline: None,
            repeats: Vec::new(),
        };
        app.chats.entry(key).or_default().msgs.push(m);
        id
    }

    fn log_rx(app: &mut App, raw: Vec<u8>) {
        let mut f = vec![proto::PUSH_LOG_RX_DATA, 24, (-80i8) as u8];
        f.extend(raw);
        app.on_frame(&f);
    }

    #[test]
    fn channel_post_repeats_are_matched() {
        let mut app = test_app();
        let secret = util::hashtag_secret("#test");
        app.channels.push(Channel { idx: 1, name: "#test".into(), secret });
        let key = ChatKey::Channel(secret);
        let id = outgoing(&mut app, key, 5000, "hello mesh");
        let other = outgoing(&mut app, key, 5001, "another");
        let payload = proto::tests::make_group_payload(&secret, 5000, "Desk Node: hello mesh");
        // flood GRP_TXT relayed by 5a then 7c (we hear 7c's retransmission)
        let mut raw = vec![(proto::PAYLOAD_GRP_TXT << 2) | 1, 2, 0x5a, 0x7c];
        raw.extend(&payload);
        log_rx(&mut app, raw.clone());
        log_rx(&mut app, raw); // the same relay again is not double-counted
        let mut raw2 = vec![(proto::PAYLOAD_GRP_TXT << 2) | 1, 1, 0x5a];
        raw2.extend(&payload);
        log_rx(&mut app, raw2);
        // someone else's post on the same channel must not count
        let mut raw3 = vec![(proto::PAYLOAD_GRP_TXT << 2) | 1, 1, 0x33];
        raw3.extend(proto::tests::make_group_payload(&secret, 5000, "Bob: hello mesh"));
        log_rx(&mut app, raw3);
        let m = app.find_message(key, id).unwrap();
        assert_eq!(m.repeats.iter().map(|r| r.0.clone()).collect::<Vec<_>>(), vec![vec![0x7c], vec![0x5a]]);
        assert_eq!(m.repeats[0].1, 6.0);
        assert!(app.find_message(key, other).unwrap().repeats.is_empty());
    }

    #[test]
    fn direct_message_repeats_are_matched() {
        let mut app = test_app();
        let pk = [0x42; 32];
        let key = ChatKey::Contact(pk);
        let id = outgoing(&mut app, key, 7000, "hi");
        // flood TXT_MSG: payload starts dest hash, src hash
        let raw = vec![(proto::PAYLOAD_TXT_MSG << 2) | 1, 1, 0x5a, 0x42, 0xD1, 0, 0, 1, 2, 3];
        log_rx(&mut app, raw);
        // a message from someone else to the same contact is ignored
        log_rx(&mut app, vec![(proto::PAYLOAD_TXT_MSG << 2) | 1, 1, 0x5a, 0x42, 0x99, 0, 0]);
        let m = app.find_message(key, id).unwrap();
        assert_eq!(m.repeats.len(), 1);
        assert_eq!(m.repeats[0].0, vec![0x5a]);
    }
    #[test]
    fn urls() {
        assert_eq!(url_decode("a%20b+c%2Fd"), "a b c/d");
        assert_eq!(url_decode(&url_encode("Hello #mesh/1")), "Hello #mesh/1");
        assert_eq!(url_decode("100%"), "100%");
    }
}
