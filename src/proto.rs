//! MeshCore companion-radio protocol (firmware `examples/companion_radio`).
//!
//! Every frame starts with a one-byte code. Over USB serial, the app sends frames as
//! `'<' len_lo len_hi payload` and the radio sends them as `'>' len_lo len_hi payload`.
//! Over BLE (Nordic UART service), each write or notification carries exactly one frame.
//! All integers are little-endian.

pub const APP_PROTOCOL_VER: u8 = 3;

// ---- commands (app -> radio) ----
pub const CMD_APP_START: u8 = 1;
pub const CMD_SEND_TXT_MSG: u8 = 2;
pub const CMD_SEND_CHANNEL_TXT_MSG: u8 = 3;
pub const CMD_GET_CONTACTS: u8 = 4;
pub const CMD_SET_DEVICE_TIME: u8 = 6;
pub const CMD_SEND_SELF_ADVERT: u8 = 7;
pub const CMD_SET_ADVERT_NAME: u8 = 8;
pub const CMD_ADD_UPDATE_CONTACT: u8 = 9;
pub const CMD_SYNC_NEXT_MESSAGE: u8 = 10;
pub const CMD_SET_RADIO_PARAMS: u8 = 11;
pub const CMD_SET_RADIO_TX_POWER: u8 = 12;
pub const CMD_RESET_PATH: u8 = 13;
pub const CMD_SET_ADVERT_LATLON: u8 = 14;
pub const CMD_REMOVE_CONTACT: u8 = 15;
pub const CMD_SHARE_CONTACT: u8 = 16;
pub const CMD_EXPORT_CONTACT: u8 = 17;
pub const CMD_IMPORT_CONTACT: u8 = 18;
pub const CMD_REBOOT: u8 = 19;
pub const CMD_GET_BATT_AND_STORAGE: u8 = 20;
pub const CMD_DEVICE_QUERY: u8 = 22;
pub const CMD_SEND_LOGIN: u8 = 26;
pub const CMD_SEND_STATUS_REQ: u8 = 27;
pub const CMD_LOGOUT: u8 = 29;
pub const CMD_GET_CHANNEL: u8 = 31;
pub const CMD_SET_CHANNEL: u8 = 32;
pub const CMD_SEND_TRACE_PATH: u8 = 36;
pub const CMD_SET_DEVICE_PIN: u8 = 37;
pub const CMD_SET_OTHER_PARAMS: u8 = 38;
pub const CMD_SEND_TELEMETRY_REQ: u8 = 39;
pub const CMD_SEND_PATH_DISCOVERY_REQ: u8 = 52;
pub const CMD_GET_STATS: u8 = 56;
pub const CMD_GET_DEVICE_TIME: u8 = 5;
pub const CMD_SET_TUNING_PARAMS: u8 = 21;
pub const CMD_EXPORT_PRIVATE_KEY: u8 = 23;
pub const CMD_IMPORT_PRIVATE_KEY: u8 = 24;
pub const CMD_GET_CONTACT_BY_KEY: u8 = 30;
pub const CMD_GET_CUSTOM_VARS: u8 = 40;
pub const CMD_SET_CUSTOM_VAR: u8 = 41;
pub const CMD_GET_ADVERT_PATH: u8 = 42;
pub const CMD_GET_TUNING_PARAMS: u8 = 43;
pub const CMD_SEND_BINARY_REQ: u8 = 50;
pub const CMD_FACTORY_RESET: u8 = 51;
pub const CMD_SET_AUTOADD_CONFIG: u8 = 58;
pub const CMD_GET_AUTOADD_CONFIG: u8 = 59;
pub const CMD_GET_ALLOWED_REPEAT_FREQ: u8 = 60;
pub const CMD_SET_PATH_HASH_MODE: u8 = 61;
pub const CMD_SET_DEFAULT_FLOOD_SCOPE: u8 = 63;
pub const CMD_GET_DEFAULT_FLOOD_SCOPE: u8 = 64;
pub const CMD_RUN_CLI_COMMAND: u8 = 66;

/// Auto-add configuration bits (CMD_SET_AUTOADD_CONFIG).
pub const AUTOADD_OVERWRITE_OLDEST: u8 = 1 << 0;
pub const AUTOADD_CHAT: u8 = 1 << 1;
pub const AUTOADD_REPEATER: u8 = 1 << 2;
pub const AUTOADD_ROOM: u8 = 1 << 3;
pub const AUTOADD_SENSOR: u8 = 1 << 4;

/// Request types for CMD_SEND_BINARY_REQ to repeaters / room servers.
pub const REQ_GET_NEIGHBOURS: u8 = 0x06;
pub const REQ_GET_OWNER_INFO: u8 = 0x07;

// ---- responses ----
pub const RESP_OK: u8 = 0;
pub const RESP_ERR: u8 = 1;
pub const RESP_CONTACTS_START: u8 = 2;
pub const RESP_CONTACT: u8 = 3;
pub const RESP_END_OF_CONTACTS: u8 = 4;
pub const RESP_SELF_INFO: u8 = 5;
pub const RESP_SENT: u8 = 6;
pub const RESP_CONTACT_MSG: u8 = 7;
pub const RESP_CHANNEL_MSG: u8 = 8;
pub const RESP_NO_MORE_MESSAGES: u8 = 10;
pub const RESP_EXPORT_CONTACT: u8 = 11;
pub const RESP_BATT_AND_STORAGE: u8 = 12;
pub const RESP_DEVICE_INFO: u8 = 13;
pub const RESP_DISABLED: u8 = 15;
pub const RESP_CONTACT_MSG_V3: u8 = 16;
pub const RESP_CHANNEL_MSG_V3: u8 = 17;
pub const RESP_CHANNEL_INFO: u8 = 18;
pub const RESP_STATS: u8 = 24;
pub const RESP_CHANNEL_DATA: u8 = 27;
pub const RESP_CURR_TIME: u8 = 9;
pub const RESP_PRIVATE_KEY: u8 = 14;
pub const RESP_CUSTOM_VARS: u8 = 21;
pub const RESP_ADVERT_PATH: u8 = 22;
pub const RESP_TUNING_PARAMS: u8 = 23;
pub const RESP_AUTOADD_CONFIG: u8 = 25;
pub const RESP_ALLOWED_REPEAT_FREQ: u8 = 26;
pub const RESP_DEFAULT_FLOOD_SCOPE: u8 = 28;
pub const RESP_CLI_REPLY: u8 = 29;

// ---- pushes (radio -> app, unsolicited) ----
pub const PUSH_ADVERT: u8 = 0x80;
pub const PUSH_PATH_UPDATED: u8 = 0x81;
pub const PUSH_SEND_CONFIRMED: u8 = 0x82;
pub const PUSH_MSG_WAITING: u8 = 0x83;
pub const PUSH_LOGIN_SUCCESS: u8 = 0x85;
pub const PUSH_LOGIN_FAIL: u8 = 0x86;
pub const PUSH_STATUS_RESPONSE: u8 = 0x87;
pub const PUSH_LOG_RX_DATA: u8 = 0x88;
pub const PUSH_TRACE_DATA: u8 = 0x89;
pub const PUSH_NEW_ADVERT: u8 = 0x8A;
pub const PUSH_TELEMETRY_RESPONSE: u8 = 0x8B;
pub const PUSH_PATH_DISCOVERY_RESPONSE: u8 = 0x8D;
pub const PUSH_CONTACT_DELETED: u8 = 0x8F;
pub const PUSH_CONTACTS_FULL: u8 = 0x90;
pub const PUSH_BINARY_RESPONSE: u8 = 0x8C;

pub const TXT_PLAIN: u8 = 0;
pub const TXT_CLI_DATA: u8 = 1;
pub const TXT_SIGNED_PLAIN: u8 = 2;

pub const ADV_CHAT: u8 = 1;
pub const ADV_REPEATER: u8 = 2;
pub const ADV_ROOM: u8 = 3;
pub const ADV_SENSOR: u8 = 4;

pub const OUT_PATH_UNKNOWN: u8 = 0xFF;

pub fn err_text(code: u8) -> &'static str {
    match code {
        1 => "unsupported command",
        2 => "not found",
        3 => "table full / busy",
        4 => "bad state",
        5 => "file I/O error",
        6 => "illegal argument",
        _ => "error",
    }
}

pub fn type_name(t: u8) -> &'static str {
    match t {
        ADV_CHAT => "Companion",
        ADV_REPEATER => "Repeater",
        ADV_ROOM => "Room server",
        ADV_SENSOR => "Sensor",
        _ => "Unknown",
    }
}

/// Number of hops and bytes-per-hop encoded in a path_len byte (firmware v1.11+).
pub fn path_dims(path_len: u8) -> (usize, usize) {
    ((path_len & 63) as usize, ((path_len >> 6) + 1) as usize)
}

// ---------------------------------------------------------------- parsed types

#[derive(Clone, Debug, Default)]
pub struct DeviceInfo {
    pub fw_ver: u8,
    pub max_contacts: u16,
    pub max_channels: u8,
    pub ble_pin: u32,
    pub build_date: String,
    pub model: String,
    pub version: String,
    /// Client repeat mode (firmware protocol v9+).
    pub repeat: Option<bool>,
    /// Path hash mode: hash size is mode + 1 bytes (v10+).
    pub path_hash_mode: Option<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct SelfInfo {
    #[allow(dead_code)]
    pub adv_type: u8,
    pub tx_power: i8,
    pub max_tx_power: i8,
    pub pubkey: [u8; 32],
    pub lat: f64,
    pub lon: f64,
    pub multi_acks: u8,
    pub advert_loc_policy: u8,
    pub telemetry_modes: u8,
    pub manual_add_contacts: u8,
    pub freq_khz: u32,
    pub bw_hz: u32,
    pub sf: u8,
    pub cr: u8,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Contact {
    pub pubkey: [u8; 32],
    pub kind: u8,
    pub flags: u8,
    pub out_path_len: u8,
    pub out_path: [u8; 64],
    pub name: String,
    pub last_advert: u32,
    pub lat: f64,
    pub lon: f64,
    #[allow(dead_code)]
    pub lastmod: u32,
}

impl Contact {
    pub fn is_favourite(&self) -> bool {
        self.flags & 1 != 0
    }
    pub fn path_bytes(&self) -> &[u8] {
        if self.out_path_len == OUT_PATH_UNKNOWN {
            return &[];
        }
        let (n, sz) = path_dims(self.out_path_len);
        &self.out_path[..(n * sz).min(64)]
    }
    pub fn route_text(&self) -> String {
        if self.out_path_len == OUT_PATH_UNKNOWN {
            return "flood".into();
        }
        let (n, sz) = path_dims(self.out_path_len);
        if n == 0 {
            return "direct".into();
        }
        let hops: Vec<String> = self.path_bytes().chunks(sz).map(crate::util::hex).collect();
        format!("{n} hop{}: {}", if n == 1 { "" } else { "s" }, hops.join(","))
    }
}

#[derive(Clone, Debug)]
pub struct RxMsg {
    /// Channel index for channel messages.
    pub channel: Option<u8>,
    /// Sender public-key prefix for direct messages.
    pub prefix: [u8; 6],
    pub path_len: u8,
    pub txt_type: u8,
    pub timestamp: u32,
    /// For signed (room server) messages: 4-byte prefix of the original author.
    pub author: Option<[u8; 4]>,
    pub text: String,
    pub snr: Option<f32>,
}

#[derive(Clone, Debug)]
pub enum Frame {
    Ok,
    Err(u8),
    Disabled,
    ContactsStart,
    Contact(Contact),
    EndOfContacts,
    SelfInfo(SelfInfo),
    Sent { flood: bool, tag: u32, timeout_ms: u32 },
    Msg(RxMsg),
    ChannelData,
    NoMoreMessages,
    ExportContact(Vec<u8>),
    Batt { mv: u16, used_kb: u32, total_kb: u32 },
    DeviceInfo(DeviceInfo),
    ChannelInfo { idx: u8, name: String, secret: [u8; 16] },
    Stats(u8, Vec<u8>),
    CurrTime(u32),
    PrivateKey(Vec<u8>),
    CustomVars(String),
    AdvertPath { ts: u32, path: Vec<u8>, hash_size: usize },
    Tuning { rx_delay: u32, airtime: u32 },
    AutoAdd { config: u8, max_hops: u8 },
    RepeatFreqs(Vec<(u32, u32)>),
    DefaultScope(Option<(String, [u8; 16])>),
    CliReply(String),
    BinaryResponse { tag: u32, data: Vec<u8> },
    // pushes
    Advert([u8; 32]),
    PathUpdated([u8; 32]),
    SendConfirmed { ack: u32, rtt_ms: u32 },
    MsgWaiting,
    LoginSuccess { admin: bool, prefix: [u8; 6] },
    LoginFail { prefix: [u8; 6] },
    StatusResponse { prefix: [u8; 6], data: Vec<u8> },
    LogRx { snr: f32, rssi: i8, raw: Vec<u8> },
    Trace { tag: u32, flags: u8, hashes: Vec<u8>, snrs: Vec<i8>, final_snr: f32 },
    NewAdvert(Contact),
    Telemetry { prefix: [u8; 6], data: Vec<u8> },
    PathDiscovery { prefix: [u8; 6], out_path: Vec<u8>, in_path: Vec<u8> },
    ContactDeleted([u8; 32]),
    ContactsFull,
    Other(u8),
}

impl Frame {
    pub fn is_push(code: u8) -> bool {
        code >= 0x80
    }
}

// ---------------------------------------------------------------- reader

struct Rd<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Rd<'a> {
    fn new(b: &'a [u8]) -> Self {
        Rd { b, i: 0 }
    }
    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.i..self.i + n)?;
        self.i += n;
        Some(s)
    }
    fn arr<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.bytes(N).map(|s| s.try_into().unwrap())
    }
    fn u8(&mut self) -> Option<u8> {
        self.bytes(1).map(|s| s[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.arr().map(u16::from_le_bytes)
    }
    fn u32(&mut self) -> Option<u32> {
        self.arr().map(u32::from_le_bytes)
    }
    fn i32(&mut self) -> Option<i32> {
        self.arr().map(i32::from_le_bytes)
    }
    fn rest(&mut self) -> &'a [u8] {
        let s = &self.b[self.i.min(self.b.len())..];
        self.i = self.b.len();
        s
    }
    fn cstr(&mut self, n: usize) -> Option<String> {
        self.bytes(n).map(cstr)
    }
}

/// Decode a NUL-padded/terminated string.
pub fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

fn parse_contact(r: &mut Rd) -> Option<Contact> {
    Some(Contact {
        pubkey: r.arr()?,
        kind: r.u8()?,
        flags: r.u8()?,
        out_path_len: r.u8()?,
        out_path: r.arr()?,
        name: r.cstr(32)?,
        last_advert: r.u32()?,
        lat: r.i32().unwrap_or(0) as f64 / 1e6,
        lon: r.i32().unwrap_or(0) as f64 / 1e6,
        lastmod: r.u32().unwrap_or(0),
    })
}

fn parse_msg(code: u8, r: &mut Rd) -> Option<RxMsg> {
    let v3 = matches!(code, RESP_CONTACT_MSG_V3 | RESP_CHANNEL_MSG_V3);
    let snr = if v3 {
        let s = r.u8()? as i8 as f32 / 4.0;
        r.bytes(2)?;
        Some(s)
    } else {
        None
    };
    let is_channel = matches!(code, RESP_CHANNEL_MSG | RESP_CHANNEL_MSG_V3);
    let (channel, prefix) = if is_channel { (Some(r.u8()?), [0; 6]) } else { (None, r.arr()?) };
    let path_len = r.u8()?;
    let txt_type = r.u8()?;
    let timestamp = r.u32()?;
    let author = if !is_channel && txt_type == TXT_SIGNED_PLAIN { Some(r.arr()?) } else { None };
    let text = String::from_utf8_lossy(r.rest()).trim_end_matches('\0').to_string();
    Some(RxMsg { channel, prefix, path_len, txt_type, timestamp, author, text, snr })
}

pub fn parse(b: &[u8]) -> Option<Frame> {
    let mut r = Rd::new(b);
    let code = r.u8()?;
    Some(match code {
        RESP_OK => Frame::Ok,
        RESP_ERR => Frame::Err(r.u8().unwrap_or(0)),
        RESP_DISABLED => Frame::Disabled,
        RESP_CONTACTS_START => Frame::ContactsStart,
        RESP_CONTACT => Frame::Contact(parse_contact(&mut r)?),
        RESP_END_OF_CONTACTS => Frame::EndOfContacts,
        RESP_SELF_INFO => Frame::SelfInfo(SelfInfo {
            adv_type: r.u8()?,
            tx_power: r.u8()? as i8,
            max_tx_power: r.u8()? as i8,
            pubkey: r.arr()?,
            lat: r.i32()? as f64 / 1e6,
            lon: r.i32()? as f64 / 1e6,
            multi_acks: r.u8()?,
            advert_loc_policy: r.u8()?,
            telemetry_modes: r.u8()?,
            manual_add_contacts: r.u8()?,
            freq_khz: r.u32()?,
            bw_hz: r.u32()?,
            sf: r.u8()?,
            cr: r.u8()?,
            name: String::from_utf8_lossy(r.rest()).trim_end_matches('\0').to_string(),
        }),
        RESP_SENT => Frame::Sent { flood: r.u8()? == 1, tag: r.u32()?, timeout_ms: r.u32()? },
        RESP_CONTACT_MSG | RESP_CHANNEL_MSG | RESP_CONTACT_MSG_V3 | RESP_CHANNEL_MSG_V3 => {
            Frame::Msg(parse_msg(code, &mut r)?)
        }
        RESP_CHANNEL_DATA => Frame::ChannelData,
        RESP_NO_MORE_MESSAGES => Frame::NoMoreMessages,
        RESP_EXPORT_CONTACT => Frame::ExportContact(r.rest().to_vec()),
        RESP_BATT_AND_STORAGE => Frame::Batt {
            mv: r.u16()?,
            used_kb: r.u32().unwrap_or(0),
            total_kb: r.u32().unwrap_or(0),
        },
        RESP_DEVICE_INFO => {
            let fw_ver = r.u8()?;
            let max_contacts = r.u8().unwrap_or(0) as u16 * 2;
            let max_channels = r.u8().unwrap_or(0);
            Frame::DeviceInfo(DeviceInfo {
                fw_ver,
                max_contacts,
                max_channels,
                ble_pin: r.u32().unwrap_or(0),
                build_date: r.cstr(12).unwrap_or_default(),
                model: r.cstr(40).unwrap_or_default(),
                version: r.cstr(20).unwrap_or_default(),
                repeat: r.u8().map(|v| v != 0),
                path_hash_mode: r.u8(),
            })
        }
        RESP_CHANNEL_INFO => Frame::ChannelInfo { idx: r.u8()?, name: r.cstr(32)?, secret: r.arr()? },
        RESP_STATS => Frame::Stats(r.u8()?, r.rest().to_vec()),
        RESP_CURR_TIME => Frame::CurrTime(r.u32()?),
        RESP_PRIVATE_KEY => Frame::PrivateKey(r.rest().to_vec()),
        RESP_CUSTOM_VARS => Frame::CustomVars(String::from_utf8_lossy(r.rest()).trim_end_matches('\0').to_string()),
        RESP_ADVERT_PATH => {
            let ts = r.u32()?;
            let pl = r.u8()?;
            let (n, sz) = path_dims(pl);
            Frame::AdvertPath { ts, path: r.bytes(n * sz)?.to_vec(), hash_size: sz }
        }
        RESP_TUNING_PARAMS => Frame::Tuning { rx_delay: r.u32()?, airtime: r.u32()? },
        RESP_AUTOADD_CONFIG => Frame::AutoAdd { config: r.u8()?, max_hops: r.u8().unwrap_or(0) },
        RESP_ALLOWED_REPEAT_FREQ => {
            let mut v = Vec::new();
            while let (Some(a), Some(b)) = (r.u32(), r.u32()) {
                v.push((a, b));
            }
            Frame::RepeatFreqs(v)
        }
        RESP_DEFAULT_FLOOD_SCOPE => match (r.cstr(31), r.arr::<16>()) {
            (Some(n), Some(k)) if !n.is_empty() => Frame::DefaultScope(Some((n, k))),
            _ => Frame::DefaultScope(None),
        },
        RESP_CLI_REPLY => Frame::CliReply(String::from_utf8_lossy(r.rest()).trim_end_matches('\0').to_string()),
        PUSH_ADVERT => Frame::Advert(r.arr()?),
        PUSH_PATH_UPDATED => Frame::PathUpdated(r.arr()?),
        PUSH_SEND_CONFIRMED => Frame::SendConfirmed { ack: r.u32()?, rtt_ms: r.u32().unwrap_or(0) },
        PUSH_MSG_WAITING => Frame::MsgWaiting,
        PUSH_LOGIN_SUCCESS => Frame::LoginSuccess { admin: r.u8()? & 1 != 0, prefix: r.arr()? },
        PUSH_LOGIN_FAIL => {
            r.u8()?;
            Frame::LoginFail { prefix: r.arr()? }
        }
        PUSH_STATUS_RESPONSE => {
            r.u8()?;
            Frame::StatusResponse { prefix: r.arr()?, data: r.rest().to_vec() }
        }
        PUSH_TELEMETRY_RESPONSE => {
            r.u8()?;
            Frame::Telemetry { prefix: r.arr()?, data: r.rest().to_vec() }
        }
        PUSH_LOG_RX_DATA => Frame::LogRx {
            snr: r.u8()? as i8 as f32 / 4.0,
            rssi: r.u8()? as i8,
            raw: r.rest().to_vec(),
        },
        PUSH_TRACE_DATA => {
            r.u8()?;
            let path_len = r.u8()? as usize;
            let flags = r.u8()?;
            let tag = r.u32()?;
            let _auth = r.u32()?;
            let hashes = r.bytes(path_len)?.to_vec();
            let snrs = r.bytes(path_len >> (flags & 3))?.iter().map(|&b| b as i8).collect();
            let final_snr = r.u8()? as i8 as f32 / 4.0;
            Frame::Trace { tag, flags, hashes, snrs, final_snr }
        }
        PUSH_NEW_ADVERT => Frame::NewAdvert(parse_contact(&mut r)?),
        PUSH_PATH_DISCOVERY_RESPONSE => {
            r.u8()?;
            let prefix = r.arr()?;
            let ol = r.u8()?;
            let (n, s) = path_dims(ol);
            let out_path = r.bytes(n * s)?.to_vec();
            let il = r.u8()?;
            let (n, s) = path_dims(il);
            let in_path = r.bytes(n * s)?.to_vec();
            Frame::PathDiscovery { prefix, out_path, in_path }
        }
        PUSH_CONTACT_DELETED => Frame::ContactDeleted(r.arr()?),
        PUSH_CONTACTS_FULL => Frame::ContactsFull,
        PUSH_BINARY_RESPONSE => {
            r.u8()?;
            Frame::BinaryResponse { tag: r.u32()?, data: r.rest().to_vec() }
        }
        c => Frame::Other(c),
    })
}

// ---------------------------------------------------------------- command builders

fn cmd(code: u8) -> Vec<u8> {
    let mut v = Vec::with_capacity(48);
    v.push(code);
    v
}

pub fn device_query() -> Vec<u8> {
    vec![CMD_DEVICE_QUERY, APP_PROTOCOL_VER]
}
pub fn app_start() -> Vec<u8> {
    let mut v = cmd(CMD_APP_START);
    v.push(1); // app ver
    v.extend_from_slice(&[0; 6]);
    v.extend_from_slice(b"meshcore-desktop");
    v
}
pub fn get_contacts(since: u32) -> Vec<u8> {
    let mut v = cmd(CMD_GET_CONTACTS);
    if since > 0 {
        v.extend_from_slice(&since.to_le_bytes());
    }
    v
}
pub fn set_time(t: u32) -> Vec<u8> {
    let mut v = cmd(CMD_SET_DEVICE_TIME);
    v.extend_from_slice(&t.to_le_bytes());
    v
}
pub fn simple(code: u8) -> Vec<u8> {
    vec![code]
}
pub fn self_advert(flood: bool) -> Vec<u8> {
    vec![CMD_SEND_SELF_ADVERT, flood as u8]
}
pub fn get_channel(idx: u8) -> Vec<u8> {
    vec![CMD_GET_CHANNEL, idx]
}
pub fn set_channel(idx: u8, name: &str, secret: &[u8; 16]) -> Vec<u8> {
    let mut v = vec![CMD_SET_CHANNEL, idx];
    let mut n = [0u8; 32];
    let nb = truncate_utf8(name, 31).as_bytes();
    n[..nb.len()].copy_from_slice(nb);
    v.extend_from_slice(&n);
    v.extend_from_slice(secret);
    v
}
pub fn send_txt(txt_type: u8, attempt: u8, ts: u32, pubkey: &[u8; 32], text: &str) -> Vec<u8> {
    let mut v = vec![CMD_SEND_TXT_MSG, txt_type, attempt];
    v.extend_from_slice(&ts.to_le_bytes());
    v.extend_from_slice(&pubkey[..6]);
    v.extend_from_slice(text.as_bytes());
    v
}
pub fn send_channel_txt(idx: u8, ts: u32, text: &str) -> Vec<u8> {
    let mut v = vec![CMD_SEND_CHANNEL_TXT_MSG, TXT_PLAIN, idx];
    v.extend_from_slice(&ts.to_le_bytes());
    v.extend_from_slice(text.as_bytes());
    v
}
pub fn with_key(code: u8, pubkey: &[u8; 32]) -> Vec<u8> {
    let mut v = cmd(code);
    v.extend_from_slice(pubkey);
    v
}
pub fn login(pubkey: &[u8; 32], password: &str) -> Vec<u8> {
    let mut v = with_key(CMD_SEND_LOGIN, pubkey);
    v.extend_from_slice(password.as_bytes());
    v
}
pub fn telemetry_req(pubkey: &[u8; 32]) -> Vec<u8> {
    let mut v = vec![CMD_SEND_TELEMETRY_REQ, 0, 0, 0];
    v.extend_from_slice(pubkey);
    v
}
pub fn path_discovery(pubkey: &[u8; 32]) -> Vec<u8> {
    let mut v = vec![CMD_SEND_PATH_DISCOVERY_REQ, 0];
    v.extend_from_slice(pubkey);
    v
}
pub fn trace(tag: u32, flags: u8, path: &[u8]) -> Vec<u8> {
    let mut v = cmd(CMD_SEND_TRACE_PATH);
    v.extend_from_slice(&tag.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.push(flags);
    v.extend_from_slice(path);
    v
}
pub fn update_contact(c: &Contact) -> Vec<u8> {
    let mut v = with_key(CMD_ADD_UPDATE_CONTACT, &c.pubkey);
    v.push(c.kind);
    v.push(c.flags);
    v.push(c.out_path_len);
    v.extend_from_slice(&c.out_path);
    let mut n = [0u8; 32];
    let nb = truncate_utf8(&c.name, 31).as_bytes();
    n[..nb.len()].copy_from_slice(nb);
    v.extend_from_slice(&n);
    v.extend_from_slice(&c.last_advert.to_le_bytes());
    v.extend_from_slice(&((c.lat * 1e6).round() as i32).to_le_bytes());
    v.extend_from_slice(&((c.lon * 1e6).round() as i32).to_le_bytes());
    v
}
pub fn import_contact(advert: &[u8]) -> Vec<u8> {
    let mut v = cmd(CMD_IMPORT_CONTACT);
    v.extend_from_slice(advert);
    v
}
pub fn export_self() -> Vec<u8> {
    vec![CMD_EXPORT_CONTACT]
}
pub fn set_name(name: &str) -> Vec<u8> {
    let mut v = cmd(CMD_SET_ADVERT_NAME);
    v.extend_from_slice(truncate_utf8(name, 31).as_bytes());
    v
}
pub fn set_latlon(lat: f64, lon: f64) -> Vec<u8> {
    let mut v = cmd(CMD_SET_ADVERT_LATLON);
    v.extend_from_slice(&((lat * 1e6).round() as i32).to_le_bytes());
    v.extend_from_slice(&((lon * 1e6).round() as i32).to_le_bytes());
    v
}
pub fn set_radio(freq_khz: u32, bw_hz: u32, sf: u8, cr: u8) -> Vec<u8> {
    let mut v = cmd(CMD_SET_RADIO_PARAMS);
    v.extend_from_slice(&freq_khz.to_le_bytes());
    v.extend_from_slice(&bw_hz.to_le_bytes());
    v.push(sf);
    v.push(cr);
    v
}
pub fn set_tx_power(dbm: i8) -> Vec<u8> {
    vec![CMD_SET_RADIO_TX_POWER, dbm as u8]
}
pub fn set_other(manual_add: u8, telemetry_modes: u8, advert_loc_policy: u8, multi_acks: u8) -> Vec<u8> {
    vec![CMD_SET_OTHER_PARAMS, manual_add, telemetry_modes, advert_loc_policy, multi_acks]
}
pub fn set_pin(pin: u32) -> Vec<u8> {
    let mut v = cmd(CMD_SET_DEVICE_PIN);
    v.extend_from_slice(&pin.to_le_bytes());
    v
}
pub fn reboot() -> Vec<u8> {
    let mut v = cmd(CMD_REBOOT);
    v.extend_from_slice(b"reboot");
    v
}
pub fn set_radio_repeat(freq_khz: u32, bw_hz: u32, sf: u8, cr: u8, repeat: bool) -> Vec<u8> {
    let mut v = set_radio(freq_khz, bw_hz, sf, cr);
    v.push(repeat as u8);
    v
}
pub fn set_tuning(rx_delay_x1000: u32, airtime_x1000: u32) -> Vec<u8> {
    let mut v = cmd(CMD_SET_TUNING_PARAMS);
    v.extend_from_slice(&rx_delay_x1000.to_le_bytes());
    v.extend_from_slice(&airtime_x1000.to_le_bytes());
    v
}
pub fn set_autoadd(config: u8, max_hops: u8) -> Vec<u8> {
    vec![CMD_SET_AUTOADD_CONFIG, config, max_hops]
}
pub fn set_path_hash_mode(mode: u8) -> Vec<u8> {
    vec![CMD_SET_PATH_HASH_MODE, 0, mode]
}
/// Set (or clear, with `None`) the default region / flood scope.
pub fn set_default_scope(scope: Option<(&str, &[u8; 16])>) -> Vec<u8> {
    let mut v = cmd(CMD_SET_DEFAULT_FLOOD_SCOPE);
    if let Some((name, key)) = scope {
        let mut n = [0u8; 31];
        let nb = truncate_utf8(name, 30).as_bytes();
        n[..nb.len()].copy_from_slice(nb);
        v.extend_from_slice(&n);
        v.extend_from_slice(key);
    }
    v
}
pub fn set_custom_var(name: &str, value: &str) -> Vec<u8> {
    let mut v = cmd(CMD_SET_CUSTOM_VAR);
    v.extend_from_slice(format!("{name}:{value}").as_bytes());
    v
}
pub fn get_advert_path(pubkey: &[u8; 32]) -> Vec<u8> {
    let mut v = vec![CMD_GET_ADVERT_PATH, 0];
    v.extend_from_slice(pubkey);
    v
}
pub fn binary_req(pubkey: &[u8; 32], data: &[u8]) -> Vec<u8> {
    let mut v = with_key(CMD_SEND_BINARY_REQ, pubkey);
    v.extend_from_slice(data);
    v
}
/// Ask a repeater for up to `count` neighbours, strongest first, with 4-byte key prefixes.
pub fn neighbours_req(pubkey: &[u8; 32], count: u8) -> Vec<u8> {
    let r = crate::util::rand_u32().to_le_bytes();
    binary_req(pubkey, &[REQ_GET_NEIGHBOURS, 0, count, 0, 0, 2, 4, r[0], r[1], r[2], r[3]])
}
pub fn owner_req(pubkey: &[u8; 32]) -> Vec<u8> {
    let r = crate::util::rand_u32().to_le_bytes();
    binary_req(pubkey, &[REQ_GET_OWNER_INFO, r[0], r[1], r[2], r[3]])
}
pub fn self_telemetry_req() -> Vec<u8> {
    vec![CMD_SEND_TELEMETRY_REQ, 0, 0, 0]
}
pub fn import_private_key(key: &[u8; 64]) -> Vec<u8> {
    let mut v = cmd(CMD_IMPORT_PRIVATE_KEY);
    v.extend_from_slice(key);
    v
}
pub fn factory_reset() -> Vec<u8> {
    let mut v = cmd(CMD_FACTORY_RESET);
    v.extend_from_slice(b"reset");
    v
}
pub fn run_cli(text: &str) -> Vec<u8> {
    let mut v = cmd(CMD_RUN_CLI_COMMAND);
    v.extend_from_slice(text.as_bytes());
    v
}
/// Contact update carrying a manually chosen route (`None` = flood).
pub fn set_contact_path(c: &Contact, path: Option<(&[u8], usize)>) -> Vec<u8> {
    let mut c = c.clone();
    match path {
        None => c.out_path_len = OUT_PATH_UNKNOWN,
        Some((bytes, sz)) => {
            c.out_path = [0; 64];
            c.out_path[..bytes.len()].copy_from_slice(bytes);
            c.out_path_len = (((sz - 1) as u8) << 6) | (bytes.len() / sz) as u8;
        }
    }
    update_contact(&c)
}

/// One neighbour from a REQ_GET_NEIGHBOURS reply.
#[derive(Clone, Debug)]
pub struct Neighbour {
    pub prefix: Vec<u8>,
    pub heard_secs_ago: u32,
    pub snr: f32,
}

/// Decode a neighbours reply: total count, then (prefix, secs ago, snr) entries.
pub fn decode_neighbours(d: &[u8], prefix_len: usize) -> Option<(u16, Vec<Neighbour>)> {
    let mut r = Rd::new(d);
    let total = r.u16()?;
    let n = r.u16()?;
    let mut v = Vec::new();
    for _ in 0..n {
        v.push(Neighbour { prefix: r.bytes(prefix_len)?.to_vec(), heard_secs_ago: r.u32()?, snr: r.u8()? as i8 as f32 / 4.0 });
    }
    Some((total, v))
}

/// An over-the-air packet as reported by PUSH_LOG_RX_DATA.
#[derive(Debug)]
pub struct RawPacket<'a> {
    pub payload_type: u8,
    pub hash_size: usize,
    /// Hashes of the nodes that relayed the packet so far (flood routing), in order.
    pub path: &'a [u8],
    pub payload: &'a [u8],
}

pub const PAYLOAD_TXT_MSG: u8 = 2;
pub const PAYLOAD_GRP_TXT: u8 = 5;

pub fn parse_raw(raw: &[u8]) -> Option<RawPacket<'_>> {
    let h = *raw.first()?;
    let route = h & 3;
    let mut i = 1 + if route == 0 || route == 3 { 4 } else { 0 }; // transport codes
    let pl = *raw.get(i)?;
    i += 1;
    let (n, sz) = path_dims(pl);
    let path = raw.get(i..i + n * sz)?;
    Some(RawPacket { payload_type: (h >> 2) & 15, hash_size: sz, path, payload: raw.get(i + n * sz..)? })
}

/// Try to decrypt a group-channel text packet payload with a channel secret.
/// Returns (sender timestamp, "name: text").
pub fn decrypt_group_text(payload: &[u8], secret: &[u8; 16]) -> Option<(u32, String)> {
    let hash = crate::util::sha256(secret)[0];
    if payload.len() < 3 + 16 || payload[0] != hash || (payload.len() - 3) % 16 != 0 {
        return None;
    }
    let mut key = [0u8; 32];
    key[..16].copy_from_slice(secret);
    let cipher = &payload[3..];
    if crate::util::hmac_sha256(&key, cipher)[..2] != payload[1..3] {
        return None;
    }
    let plain = crate::util::aes128_ecb_decrypt(secret, cipher);
    let ts = u32::from_le_bytes(plain.get(0..4)?.try_into().ok()?);
    let text = cstr(plain.get(5..)?);
    Some((ts, text))
}

pub fn get_stats(kind: u8) -> Vec<u8> {
    vec![CMD_GET_STATS, kind]
}

pub fn truncate_utf8(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut e = max;
    while !s.is_char_boundary(e) {
        e -= 1;
    }
    &s[..e]
}

// ---------------------------------------------------------------- serial framing

/// Wrap a frame for the USB serial link.
pub fn serial_wrap(frame: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(frame.len() + 3);
    v.push(b'<');
    v.extend_from_slice(&(frame.len() as u16).to_le_bytes());
    v.extend_from_slice(frame);
    v
}

/// Incremental decoder for `'>' len16 payload` frames from the radio.
#[derive(Default)]
pub struct SerialDeframer {
    buf: Vec<u8>,
}

impl SerialDeframer {
    pub fn push(&mut self, data: &[u8], mut out: impl FnMut(Vec<u8>)) {
        self.buf.extend_from_slice(data);
        loop {
            // resynchronise on the start marker (skips any boot/debug text)
            match self.buf.iter().position(|&b| b == b'>') {
                Some(0) => {}
                Some(p) => {
                    self.buf.drain(..p);
                }
                None => {
                    self.buf.clear();
                    return;
                }
            }
            if self.buf.len() < 3 {
                return;
            }
            let len = u16::from_le_bytes([self.buf[1], self.buf[2]]) as usize;
            if len == 0 || len > 512 {
                self.buf.drain(..1); // not a real header
                continue;
            }
            if self.buf.len() < 3 + len {
                return;
            }
            let frame = self.buf[3..3 + len].to_vec();
            self.buf.drain(..3 + len);
            out(frame);
        }
    }
}

// ---------------------------------------------------------------- remote stats decoding

/// Decode a repeater/room-server status (RepeaterStats struct) into display lines.
pub fn decode_status(d: &[u8]) -> Vec<(String, String)> {
    let mut r = Rd::new(d);
    let mut out = Vec::new();
    let mut put = |k: &str, v: Option<String>| {
        if let Some(v) = v {
            out.push((k.to_string(), v))
        }
    };
    put("Battery", r.u16().map(|v| format!("{:.2} V", v as f32 / 1000.0)));
    put("TX queue", r.u16().map(|v| v.to_string()));
    put("Noise floor", r.u16().map(|v| format!("{} dBm", v as i16)));
    put("Last RSSI", r.u16().map(|v| format!("{} dBm", v as i16)));
    put("Packets received", r.u32().map(|v| v.to_string()));
    put("Packets sent", r.u32().map(|v| v.to_string()));
    put("TX air time", r.u32().map(crate::util::duration));
    put("Uptime", r.u32().map(crate::util::duration));
    put("Sent flood", r.u32().map(|v| v.to_string()));
    put("Sent direct", r.u32().map(|v| v.to_string()));
    put("Received flood", r.u32().map(|v| v.to_string()));
    put("Received direct", r.u32().map(|v| v.to_string()));
    put("Error events", r.u16().map(|v| v.to_string()));
    put("Last SNR", r.u16().map(|v| format!("{:.2} dB", v as i16 as f32 / 4.0)));
    put("Direct duplicates", r.u16().map(|v| v.to_string()));
    put("Flood duplicates", r.u16().map(|v| v.to_string()));
    put("RX air time", r.u32().map(crate::util::duration));
    put("Receive errors", r.u32().map(|v| v.to_string()));
    out
}

/// Decode local CMD_GET_STATS replies.
pub fn decode_local_stats(kind: u8, d: &[u8]) -> Vec<(String, String)> {
    let mut r = Rd::new(d);
    let mut out = Vec::new();
    let mut put = |k: &str, v: Option<String>| {
        if let Some(v) = v {
            out.push((k.to_string(), v))
        }
    };
    match kind {
        0 => {
            put("Battery", r.u16().map(|v| format!("{:.2} V", v as f32 / 1000.0)));
            put("Uptime", r.u32().map(crate::util::duration));
            put("Error flags", r.u16().map(|v| format!("{v:#06x}")));
            put("Outbound queue", r.u8().map(|v| v.to_string()));
        }
        1 => {
            put("Noise floor", r.u16().map(|v| format!("{} dBm", v as i16)));
            put("Last RSSI", r.u8().map(|v| format!("{} dBm", v as i8)));
            put("Last SNR", r.u8().map(|v| format!("{:.2} dB", v as i8 as f32 / 4.0)));
            put("TX air time", r.u32().map(crate::util::duration));
            put("RX air time", r.u32().map(crate::util::duration));
        }
        _ => {
            for k in ["Received", "Sent", "Sent flood", "Sent direct", "Received flood", "Received direct", "Receive errors"] {
                put(k, r.u32().map(|v| v.to_string()));
            }
        }
    }
    out
}

/// Decode Cayenne LPP telemetry into display lines.
pub fn decode_lpp(d: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    let be = |s: &[u8]| s.iter().fold(0u32, |a, &b| (a << 8) | b as u32);
    let sbe = |s: &[u8]| {
        let bits = s.len() * 8;
        ((be(s) << (32 - bits)) as i32) >> (32 - bits)
    };
    while i + 2 <= d.len() {
        let ch = d[i];
        let t = d[i + 1];
        if ch == 0 && t == 0 {
            break;
        }
        i += 2;
        let size = match t {
            0 | 1 | 102 | 104 | 120 | 142 => 1,
            2 | 3 | 101 | 103 | 115 | 116 | 117 | 121 | 125 | 128 | 132 => 2,
            135 => 3,
            100 | 118 | 130 | 131 | 133 => 4,
            113 | 134 => 6,
            136 => 9,
            _ => break,
        };
        let Some(v) = d.get(i..i + size) else { break };
        i += size;
        let (name, val) = match t {
            0 => ("Digital in", be(v).to_string()),
            1 => ("Digital out", be(v).to_string()),
            2 => ("Analogue in", format!("{:.2}", sbe(v) as f32 / 100.0)),
            3 => ("Analogue out", format!("{:.2}", sbe(v) as f32 / 100.0)),
            100 => ("Generic", be(v).to_string()),
            101 => ("Illuminance", format!("{} lux", be(v))),
            102 => ("Presence", be(v).to_string()),
            103 => ("Temperature", format!("{:.1} °C", sbe(v) as f32 / 10.0)),
            104 => ("Humidity", format!("{:.1} %", be(v) as f32 / 2.0)),
            113 => ("Accelerometer", format!("{:.3}, {:.3}, {:.3} G", sbe(&v[0..2]) as f32 / 1000.0, sbe(&v[2..4]) as f32 / 1000.0, sbe(&v[4..6]) as f32 / 1000.0)),
            115 => ("Pressure", format!("{:.1} hPa", be(v) as f32 / 10.0)),
            116 => ("Voltage", format!("{:.2} V", sbe(v) as f32 / 100.0)),
            117 => ("Current", format!("{:.3} A", sbe(v) as f32 / 1000.0)),
            118 => ("Frequency", format!("{} Hz", be(v))),
            120 => ("Percentage", format!("{} %", be(v))),
            121 => ("Altitude", format!("{} m", sbe(v))),
            125 => ("Concentration", format!("{} ppm", be(v))),
            128 => ("Power", format!("{} W", be(v))),
            130 => ("Distance", format!("{:.3} m", be(v) as f32 / 1000.0)),
            131 => ("Energy", format!("{:.3} kWh", be(v) as f32 / 1000.0)),
            132 => ("Direction", format!("{}°", be(v))),
            133 => ("Time", crate::util::fmt_time(be(v) as u64)),
            134 => ("Gyrometer", format!("{:.2}, {:.2}, {:.2} °/s", sbe(&v[0..2]) as f32 / 100.0, sbe(&v[2..4]) as f32 / 100.0, sbe(&v[4..6]) as f32 / 100.0)),
            135 => ("Colour", format!("#{}", crate::util::hex(v))),
            136 => ("GPS", format!("{:.4}, {:.4}, {:.2} m", sbe(&v[0..3]) as f32 / 1e4, sbe(&v[3..6]) as f32 / 1e4, sbe(&v[6..9]) as f32 / 100.0)),
            142 => ("Switch", be(v).to_string()),
            _ => unreachable!(),
        };
        out.push((format!("{name} [ch {ch}]"), val));
    }
    out
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn deframer_handles_noise_and_splits() {
        let mut d = SerialDeframer::default();
        let mut got = Vec::new();
        d.push(b"boot junk\r\n>\x02", |f| got.push(f));
        d.push(b"\x00\x0a", |f| got.push(f));
        d.push(b"\x0b>\x01\x00\x00", |f| got.push(f));
        assert_eq!(got, vec![vec![0x0a, 0x0b], vec![0x00]]);
    }

    #[test]
    fn parse_contact_frame() {
        let mut f = vec![RESP_CONTACT];
        f.extend_from_slice(&[7; 32]);
        f.extend_from_slice(&[ADV_REPEATER, 1, 2]);
        let mut path = [0u8; 64];
        path[0] = 0xAB;
        path[1] = 0xCD;
        f.extend_from_slice(&path);
        let mut name = [0u8; 32];
        name[..4].copy_from_slice(b"Rptr");
        f.extend_from_slice(&name);
        f.extend_from_slice(&100u32.to_le_bytes());
        f.extend_from_slice(&51_500_000i32.to_le_bytes());
        f.extend_from_slice(&(-120_000i32).to_le_bytes());
        f.extend_from_slice(&5u32.to_le_bytes());
        let Some(Frame::Contact(c)) = parse(&f) else { panic!() };
        assert_eq!(c.name, "Rptr");
        assert_eq!(c.path_bytes(), &[0xAB, 0xCD]);
        assert!((c.lat - 51.5).abs() < 1e-9 && (c.lon + 0.12).abs() < 1e-9);
        assert_eq!(c.route_text(), "2 hops: ab,cd");
        // round-trip through the update builder
        let u = update_contact(&c);
        assert_eq!(&u[1..], &f[1..f.len() - 4]);
    }

    #[test]
    fn parse_v3_messages() {
        let mut f = vec![RESP_CHANNEL_MSG_V3, 0xF8, 0, 0, 2, 3, 0];
        f.extend_from_slice(&1000u32.to_le_bytes());
        f.extend_from_slice(b"Bob: hi");
        let Some(Frame::Msg(m)) = parse(&f) else { panic!() };
        assert_eq!((m.channel, m.snr, m.timestamp, m.text.as_str()), (Some(2), Some(-2.0), 1000, "Bob: hi"));

        let mut f = vec![RESP_CONTACT_MSG_V3, 20, 0, 0, 1, 2, 3, 4, 5, 6, 0xFF, TXT_SIGNED_PLAIN];
        f.extend_from_slice(&7u32.to_le_bytes());
        f.extend_from_slice(&[9, 9, 9, 9]);
        f.extend_from_slice(b"room post");
        let Some(Frame::Msg(m)) = parse(&f) else { panic!() };
        assert_eq!(m.prefix, [1, 2, 3, 4, 5, 6]);
        assert_eq!(m.author, Some([9; 4]));
        assert_eq!(m.text, "room post");
    }

    #[test]
    fn trace_push() {
        // 3 one-byte hops out and back (a,b,a): 3 hashes, 3 SNRs, final SNR
        let mut f = vec![PUSH_TRACE_DATA, 0, 3, 0];
        f.extend_from_slice(&7u32.to_le_bytes());
        f.extend_from_slice(&0u32.to_le_bytes());
        f.extend_from_slice(&[0x5a, 0x22, 0x5a, 36, 28, 0xF8, 40]);
        let Some(Frame::Trace { tag, hashes, snrs, final_snr, .. }) = parse(&f) else { panic!() };
        assert_eq!((tag, hashes, snrs, final_snr), (7, vec![0x5a, 0x22, 0x5a], vec![36, 28, -8], 10.0));
    }

    /// Build a group-text payload exactly as the firmware does (hash, 2-byte HMAC, AES-128-ECB).
    pub fn make_group_payload(secret: &[u8; 16], ts: u32, text: &str) -> Vec<u8> {
        let mut plain = ts.to_le_bytes().to_vec();
        plain.push(0);
        plain.extend_from_slice(text.as_bytes());
        while plain.len() % 16 != 0 {
            plain.push(0);
        }
        let cipher: Vec<u8> = plain.chunks(16).flat_map(|b| crate::util::aes128_encrypt_block(secret, b.try_into().unwrap())).collect();
        let mut key = [0u8; 32];
        key[..16].copy_from_slice(secret);
        let mac = crate::util::hmac_sha256(&key, &cipher);
        let mut p = vec![crate::util::sha256(secret)[0], mac[0], mac[1]];
        p.extend(cipher);
        p
    }

    #[test]
    fn group_text_decrypts_and_rejects_wrong_key() {
        let secret = crate::util::hashtag_secret("#test");
        let p = make_group_payload(&secret, 1234, "Desk Node: hello mesh");
        assert_eq!(decrypt_group_text(&p, &secret), Some((1234, "Desk Node: hello mesh".to_string())));
        assert_eq!(decrypt_group_text(&p, &crate::util::hashtag_secret("#other")), None);
        let mut bad = p.clone();
        *bad.last_mut().unwrap() ^= 1; // tampered ciphertext fails the MAC
        assert_eq!(decrypt_group_text(&bad, &secret), None);
    }

    #[test]
    fn raw_packet_with_transport_codes() {
        // header: GRP_TXT (5) with T-flood route (0) -> 4 transport-code bytes before path_len
        let raw = [5 << 2, 1, 2, 3, 4, 0x42, 0xAA, 0xBB, 0x01, 0x02, 0x09];
        let p = parse_raw(&raw).unwrap();
        // path_len 0x42: hash size 2, 2 hops
        assert_eq!((p.payload_type, p.hash_size, p.path, p.payload), (5, 2, &[0xAA, 0xBB, 0x01, 0x02][..], &[0x09][..]));
    }

    #[test]
    fn new_reply_frames() {
        let mut f = vec![RESP_ADVERT_PATH];
        f.extend_from_slice(&99u32.to_le_bytes());
        f.extend_from_slice(&[2, 0x5a, 0x7c]);
        assert!(matches!(parse(&f), Some(Frame::AdvertPath { ts: 99, ref path, hash_size: 1 }) if path == &vec![0x5a, 0x7c]));
        assert!(matches!(parse(&[RESP_AUTOADD_CONFIG, 0x07, 3]), Some(Frame::AutoAdd { config: 7, max_hops: 3 })));
        let mut n = Vec::new();
        n.extend_from_slice(&5u16.to_le_bytes());
        n.extend_from_slice(&1u16.to_le_bytes());
        n.extend_from_slice(&[1, 2, 3, 4]);
        n.extend_from_slice(&60u32.to_le_bytes());
        n.push(0xF4); // -3 dB
        let (total, list) = decode_neighbours(&n, 4).unwrap();
        assert_eq!((total, list.len(), list[0].heard_secs_ago, list[0].snr), (5, 1, 60, -3.0));
        let mut d = vec![RESP_DEVICE_INFO, 13, 175, 40];
        d.extend_from_slice(&[0; 4 + 12 + 40 + 20]);
        d.extend_from_slice(&[1, 2]);
        let Some(Frame::DeviceInfo(i)) = parse(&d) else { panic!() };
        assert_eq!((i.repeat, i.path_hash_mode), (Some(true), Some(2)));
    }

    #[test]
    fn lpp() {
        let d = [1, 116, 0x01, 0x9A, 1, 103, 0xFF, 0x38];
        let v = decode_lpp(&d);
        assert_eq!(v[0].1, "4.10 V");
        assert_eq!(v[1].1, "-20.0 °C");
    }
}
