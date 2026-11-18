//! A deliberately tiny D-Bus client (system bus, little-endian, no fd passing):
//! just enough of the wire protocol to drive BlueZ and serve a pairing agent.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub enum V {
    Y(u8),
    B(bool),
    N(i16),
    Q(u16),
    I(i32),
    U(u32),
    X(i64),
    T(u64),
    D(f64),
    S(String),
    O(String),
    G(String),
    /// Array: element signature + items.
    A(String, Vec<V>),
    /// Struct.
    R(Vec<V>),
    /// Dict entry.
    E(Box<V>, Box<V>),
    Var(Box<V>),
}

impl V {
    pub fn sig(&self) -> String {
        match self {
            V::Y(_) => "y".into(),
            V::B(_) => "b".into(),
            V::N(_) => "n".into(),
            V::Q(_) => "q".into(),
            V::I(_) => "i".into(),
            V::U(_) => "u".into(),
            V::X(_) => "x".into(),
            V::T(_) => "t".into(),
            V::D(_) => "d".into(),
            V::S(_) => "s".into(),
            V::O(_) => "o".into(),
            V::G(_) => "g".into(),
            V::A(e, _) => format!("a{e}"),
            V::R(f) => format!("({})", f.iter().map(V::sig).collect::<String>()),
            V::E(k, v) => format!("{{{}{}}}", k.sig(), v.sig()),
            V::Var(_) => "v".into(),
        }
    }
    pub fn str(&self) -> Option<&str> {
        match self {
            V::S(s) | V::O(s) | V::G(s) => Some(s),
            V::Var(v) => v.str(),
            _ => None,
        }
    }
    pub fn bool(&self) -> Option<bool> {
        match self {
            V::B(b) => Some(*b),
            V::Var(v) => v.bool(),
            _ => None,
        }
    }
    pub fn int(&self) -> Option<i64> {
        match self {
            V::Y(x) => Some(*x as i64),
            V::N(x) => Some(*x as i64),
            V::Q(x) => Some(*x as i64),
            V::I(x) => Some(*x as i64),
            V::U(x) => Some(*x as i64),
            V::X(x) => Some(*x),
            V::T(x) => Some(*x as i64),
            V::Var(v) => v.int(),
            _ => None,
        }
    }
    pub fn items(&self) -> &[V] {
        match self {
            V::A(_, v) => v,
            V::Var(v) => v.items(),
            _ => &[],
        }
    }
    pub fn bytes(&self) -> Vec<u8> {
        self.items().iter().filter_map(|x| if let V::Y(b) = x { Some(*b) } else { None }).collect()
    }
    /// Look up a key in an `a{s*}` / `a{o*}` dictionary.
    pub fn get(&self, key: &str) -> Option<&V> {
        self.items().iter().find_map(|e| match e {
            V::E(k, v) if k.str() == Some(key) => Some(&**v),
            _ => None,
        })
    }
    /// Build an `a{sv}` dictionary.
    pub fn dict(entries: Vec<(&str, V)>) -> V {
        V::A("{sv}".into(), entries.into_iter().map(|(k, v)| V::E(Box::new(V::S(k.into())), Box::new(V::Var(Box::new(v))))).collect())
    }
}

fn align_of(c: u8) -> usize {
    match c {
        b'y' | b'g' | b'v' => 1,
        b'n' | b'q' => 2,
        b'x' | b't' | b'd' | b'(' | b'{' => 8,
        _ => 4,
    }
}

fn pad(buf: &mut Vec<u8>, a: usize) {
    while buf.len() % a != 0 {
        buf.push(0);
    }
}

fn put(buf: &mut Vec<u8>, v: &V) {
    match v {
        V::Y(x) => buf.push(*x),
        V::B(x) => {
            pad(buf, 4);
            buf.extend_from_slice(&(*x as u32).to_le_bytes())
        }
        V::N(x) => {
            pad(buf, 2);
            buf.extend_from_slice(&x.to_le_bytes())
        }
        V::Q(x) => {
            pad(buf, 2);
            buf.extend_from_slice(&x.to_le_bytes())
        }
        V::I(x) => {
            pad(buf, 4);
            buf.extend_from_slice(&x.to_le_bytes())
        }
        V::U(x) => {
            pad(buf, 4);
            buf.extend_from_slice(&x.to_le_bytes())
        }
        V::X(x) => {
            pad(buf, 8);
            buf.extend_from_slice(&x.to_le_bytes())
        }
        V::T(x) => {
            pad(buf, 8);
            buf.extend_from_slice(&x.to_le_bytes())
        }
        V::D(x) => {
            pad(buf, 8);
            buf.extend_from_slice(&x.to_le_bytes())
        }
        V::S(s) | V::O(s) => {
            pad(buf, 4);
            buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
            buf.extend_from_slice(s.as_bytes());
            buf.push(0);
        }
        V::G(s) => {
            buf.push(s.len() as u8);
            buf.extend_from_slice(s.as_bytes());
            buf.push(0);
        }
        V::A(es, items) => {
            pad(buf, 4);
            let lp = buf.len();
            buf.extend_from_slice(&[0; 4]);
            pad(buf, align_of(es.as_bytes()[0]));
            let start = buf.len();
            for it in items {
                put(buf, it);
            }
            let n = (buf.len() - start) as u32;
            buf[lp..lp + 4].copy_from_slice(&n.to_le_bytes());
        }
        V::R(f) => {
            pad(buf, 8);
            for x in f {
                put(buf, x);
            }
        }
        V::E(k, v) => {
            pad(buf, 8);
            put(buf, k);
            put(buf, v);
        }
        V::Var(x) => {
            put(buf, &V::G(x.sig()));
            put(buf, x);
        }
    }
}

/// End index (exclusive) of the single complete type starting at `sig[i]`.
fn type_end(sig: &[u8], i: usize) -> Option<usize> {
    match *sig.get(i)? {
        b'a' => type_end(sig, i + 1),
        b'(' | b'{' => {
            let close = if sig[i] == b'(' { b')' } else { b'}' };
            let mut j = i + 1;
            while *sig.get(j)? != close {
                j = type_end(sig, j)?;
            }
            Some(j + 1)
        }
        _ => Some(i + 1),
    }
}

struct P<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> P<'a> {
    fn align(&mut self, a: usize) {
        self.i = self.i.div_ceil(a) * a;
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.i..self.i + n)?;
        self.i += n;
        Some(s)
    }
    fn u32(&mut self) -> Option<u32> {
        self.align(4);
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn val(&mut self, sig: &[u8]) -> Option<V> {
        Some(match sig[0] {
            b'y' => V::Y(self.take(1)?[0]),
            b'b' => V::B(self.u32()? != 0),
            b'n' | b'q' => {
                self.align(2);
                let x = u16::from_le_bytes(self.take(2)?.try_into().ok()?);
                if sig[0] == b'n' {
                    V::N(x as i16)
                } else {
                    V::Q(x)
                }
            }
            b'i' => V::I(self.u32()? as i32),
            b'u' | b'h' => V::U(self.u32()?),
            b'x' | b't' | b'd' => {
                self.align(8);
                let x = u64::from_le_bytes(self.take(8)?.try_into().ok()?);
                match sig[0] {
                    b'x' => V::X(x as i64),
                    b't' => V::T(x),
                    _ => V::D(f64::from_bits(x)),
                }
            }
            b's' | b'o' => {
                let n = self.u32()? as usize;
                let s = String::from_utf8_lossy(self.take(n)?).into_owned();
                self.take(1)?;
                if sig[0] == b's' {
                    V::S(s)
                } else {
                    V::O(s)
                }
            }
            b'g' => {
                let n = self.take(1)?[0] as usize;
                let s = String::from_utf8_lossy(self.take(n)?).into_owned();
                self.take(1)?;
                V::G(s)
            }
            b'v' => {
                let V::G(s) = self.val(b"g")? else { return None };
                let inner = self.val(s.as_bytes())?;
                V::Var(Box::new(inner))
            }
            b'a' => {
                let n = self.u32()? as usize;
                let es = &sig[1..type_end(sig, 1)?];
                self.align(align_of(es[0]));
                let end = self.i + n;
                let mut items = Vec::new();
                while self.i < end {
                    items.push(self.val(es)?);
                }
                V::A(String::from_utf8_lossy(es).into_owned(), items)
            }
            b'(' | b'{' => {
                self.align(8);
                let mut j = 1;
                let mut f = Vec::new();
                let end = type_end(sig, 0)? - 1;
                while j < end {
                    let e = type_end(sig, j)?;
                    f.push(self.val(&sig[j..e])?);
                    j = e;
                }
                if sig[0] == b'{' && f.len() == 2 {
                    let v = f.pop()?;
                    let k = f.pop()?;
                    V::E(Box::new(k), Box::new(v))
                } else {
                    V::R(f)
                }
            }
            _ => return None,
        })
    }
}

pub const METHOD_CALL: u8 = 1;
pub const METHOD_RETURN: u8 = 2;
pub const ERROR: u8 = 3;
pub const SIGNAL: u8 = 4;

#[derive(Clone, Debug, Default)]
pub struct Msg {
    pub kind: u8,
    pub serial: u32,
    pub reply_serial: Option<u32>,
    pub path: String,
    pub iface: String,
    pub member: String,
    pub error: String,
    pub dest: String,
    pub sender: String,
    pub body: Vec<V>,
}

impl Msg {
    pub fn call(dest: &str, path: &str, iface: &str, member: &str, body: Vec<V>) -> Msg {
        Msg { kind: METHOD_CALL, dest: dest.into(), path: path.into(), iface: iface.into(), member: member.into(), body, ..Default::default() }
    }
    fn encode(&self, serial: u32) -> Vec<u8> {
        let mut body = Vec::new();
        for v in &self.body {
            put(&mut body, v);
        }
        let mut fields = Vec::new();
        let mut f = |code: u8, v: V| fields.push(V::R(vec![V::Y(code), V::Var(Box::new(v))]));
        if !self.path.is_empty() {
            f(1, V::O(self.path.clone()));
        }
        if !self.iface.is_empty() {
            f(2, V::S(self.iface.clone()));
        }
        if !self.member.is_empty() {
            f(3, V::S(self.member.clone()));
        }
        if !self.error.is_empty() {
            f(4, V::S(self.error.clone()));
        }
        if let Some(r) = self.reply_serial {
            f(5, V::U(r));
        }
        if !self.dest.is_empty() {
            f(6, V::S(self.dest.clone()));
        }
        if !self.body.is_empty() {
            f(8, V::G(self.body.iter().map(V::sig).collect()));
        }
        let mut m = vec![b'l', self.kind, 0, 1];
        m.extend_from_slice(&(body.len() as u32).to_le_bytes());
        m.extend_from_slice(&serial.to_le_bytes());
        put(&mut m, &V::A("(yv)".into(), fields));
        pad(&mut m, 8);
        m.extend_from_slice(&body);
        m
    }

    /// Try to decode one message from the front of `b`; returns it and its length.
    fn decode(b: &[u8]) -> Option<Result<(Msg, usize), String>> {
        if b.len() < 16 {
            return None;
        }
        if b[0] != b'l' {
            return Some(Err("big-endian D-Bus messages not supported".into()));
        }
        let body_len = u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize;
        let fields_len = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
        let hdr_end = (16 + fields_len).div_ceil(8) * 8;
        let total = hdr_end + body_len;
        if b.len() < total {
            return None;
        }
        let mut m = Msg { kind: b[1], serial: u32::from_le_bytes(b[8..12].try_into().unwrap()), ..Default::default() };
        let mut p = P { b: &b[..hdr_end], i: 12 };
        let mut sig = String::new();
        if let Some(V::A(_, fields)) = p.val(b"a(yv)") {
            for fv in fields {
                let V::R(f) = fv else { continue };
                let (Some(V::Y(code)), Some(val)) = (f.first(), f.get(1)) else { continue };
                let s = val.str().unwrap_or("").to_string();
                match code {
                    1 => m.path = s,
                    2 => m.iface = s,
                    3 => m.member = s,
                    4 => m.error = s,
                    5 => m.reply_serial = val.int().map(|x| x as u32),
                    6 => m.dest = s,
                    7 => m.sender = s,
                    8 => sig = s,
                    _ => {}
                }
            }
        }
        let mut p = P { b: &b[hdr_end..total], i: 0 };
        let sb = sig.as_bytes();
        let mut j = 0;
        while j < sb.len() {
            let Some(e) = type_end(sb, j) else { break };
            match p.val(&sb[j..e]) {
                Some(v) => m.body.push(v),
                None => break,
            }
            j = e;
        }
        Some(Ok((m, total)))
    }
}

pub const AGENT_PATH: &str = "/meshcore/agent";

pub struct Bus {
    sock: UnixStream,
    serial: u32,
    rbuf: Vec<u8>,
    /// Incoming messages not yet consumed (signals, unrelated replies).
    pub queue: VecDeque<Msg>,
    /// When set, method calls to AGENT_PATH are answered with this passkey.
    pub agent_pin: Option<u32>,
}

impl Bus {
    pub fn system() -> Result<Bus, String> {
        let path = std::env::var("DBUS_SYSTEM_BUS_ADDRESS")
            .ok()
            .and_then(|a| a.split(';').find_map(|p| p.strip_prefix("unix:path=").map(|s| s.split(',').next().unwrap_or(s).to_string())))
            .unwrap_or_else(|| "/run/dbus/system_bus_socket".into());
        let sock = UnixStream::connect(&path)
            .or_else(|_| UnixStream::connect("/var/run/dbus/system_bus_socket"))
            .map_err(|e| format!("cannot reach the D-Bus system bus ({e}); is BlueZ running?"))?;
        let mut bus = Bus { sock, serial: 0, rbuf: Vec::new(), queue: VecDeque::new(), agent_pin: None };
        bus.auth()?;
        bus.call(Msg::call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "Hello", vec![]), 5000)?;
        Ok(bus)
    }

    fn auth(&mut self) -> Result<(), String> {
        let uid = unsafe { libc::getuid() };
        let hexuid = crate::util::hex(uid.to_string().as_bytes());
        let e = |e: std::io::Error| format!("D-Bus auth: {e}");
        self.sock.write_all(format!("\0AUTH EXTERNAL {hexuid}\r\n").as_bytes()).map_err(e)?;
        self.sock.set_read_timeout(Some(Duration::from_secs(5))).map_err(e)?;
        let mut line = Vec::new();
        let mut c = [0u8; 1];
        while !line.ends_with(b"\r\n") {
            self.sock.read_exact(&mut c).map_err(e)?;
            line.push(c[0]);
        }
        if !line.starts_with(b"OK") {
            return Err(format!("D-Bus auth rejected: {}", String::from_utf8_lossy(&line).trim()));
        }
        self.sock.write_all(b"BEGIN\r\n").map_err(e)?;
        Ok(())
    }

    pub fn send(&mut self, m: &Msg) -> Result<u32, String> {
        self.serial += 1;
        self.sock.write_all(&m.encode(self.serial)).map_err(|e| format!("D-Bus write: {e}"))?;
        Ok(self.serial)
    }

    /// Receive one message (waiting at most `ms`). Agent requests are answered internally.
    pub fn recv(&mut self, ms: u64) -> Result<Option<Msg>, String> {
        let deadline = Instant::now() + Duration::from_millis(ms.max(1));
        loop {
            if let Some(r) = Msg::decode(&self.rbuf) {
                let (m, n) = r?;
                self.rbuf.drain(..n);
                if m.kind == METHOD_CALL {
                    self.answer_call(&m)?;
                    continue;
                }
                return Ok(Some(m));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            self.sock.set_read_timeout(Some(left)).map_err(|e| e.to_string())?;
            let mut buf = [0u8; 4096];
            match self.sock.read(&mut buf) {
                Ok(0) => return Err("D-Bus connection closed".into()),
                Ok(n) => self.rbuf.extend_from_slice(&buf[..n]),
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => return Ok(None),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(format!("D-Bus read: {e}")),
            }
        }
    }

    /// Next message: queued ones first, then the socket.
    pub fn next(&mut self, ms: u64) -> Result<Option<Msg>, String> {
        if let Some(m) = self.queue.pop_front() {
            return Ok(Some(m));
        }
        self.recv(ms)
    }

    fn answer_call(&mut self, m: &Msg) -> Result<(), String> {
        let mut reply = Msg { kind: METHOD_RETURN, reply_serial: Some(m.serial), dest: m.sender.clone(), ..Default::default() };
        if m.path == AGENT_PATH && m.iface == "org.bluez.Agent1" {
            let pin = self.agent_pin.unwrap_or(123456);
            match m.member.as_str() {
                "RequestPasskey" => reply.body = vec![V::U(pin)],
                "RequestPinCode" => reply.body = vec![V::S(format!("{pin:06}"))],
                _ => {} // Release, Cancel, DisplayPasskey, RequestConfirmation, Authorize*: accept
            }
        } else {
            reply.kind = ERROR;
            reply.error = "org.freedesktop.DBus.Error.UnknownMethod".into();
            reply.body = vec![V::S("unknown method".into())];
        }
        self.send(&reply).map(|_| ())
    }

    /// Call a method and wait for its reply; D-Bus errors become `Err`.
    pub fn call(&mut self, m: Msg, timeout_ms: u64) -> Result<Msg, String> {
        let serial = self.send(&m)?;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let left = deadline.saturating_duration_since(Instant::now()).as_millis() as u64;
            if left == 0 {
                return Err(format!("{} timed out", m.member));
            }
            if let Some(r) = self.recv(left)? {
                if r.reply_serial == Some(serial) {
                    if r.kind == ERROR {
                        let detail = r.body.first().and_then(V::str).unwrap_or("");
                        return Err(format!("{} {}", r.error.trim_start_matches("org.bluez.Error."), detail).trim().to_string());
                    }
                    return Ok(r);
                }
                self.queue.push_back(r);
            }
        }
    }

    pub fn add_match(&mut self, rule: &str) -> Result<(), String> {
        self.call(
            Msg::call("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "AddMatch", vec![V::S(rule.into())]),
            5000,
        )
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut m = Msg::call("org.bluez", "/org/bluez/hci0/dev_X", "org.bluez.GattCharacteristic1", "WriteValue", vec![
            V::A("y".into(), vec![V::Y(1), V::Y(2), V::Y(3)]),
            V::dict(vec![("type", V::S("request".into())), ("offset", V::Q(0))]),
        ]);
        m.kind = METHOD_CALL;
        let bytes = m.encode(7);
        let (d, n) = Msg::decode(&bytes).unwrap().unwrap();
        assert_eq!(n, bytes.len());
        assert_eq!(d.serial, 7);
        assert_eq!(d.member, "WriteValue");
        assert_eq!(d.path, "/org/bluez/hci0/dev_X");
        assert_eq!(d.body[0].bytes(), vec![1, 2, 3]);
        assert_eq!(d.body[1].get("type").and_then(V::str), Some("request"));
        assert_eq!(d.body[1].get("offset").and_then(V::int), Some(0));
    }

    #[test]
    fn nested_dicts() {
        // a{oa{sa{sv}}} like GetManagedObjects
        let inner = V::A("{sa{sv}}".into(), vec![V::E(
            Box::new(V::S("org.bluez.Device1".into())),
            Box::new(V::dict(vec![("Name", V::S("MeshCore-x".into())), ("RSSI", V::N(-60)), ("Paired", V::B(true))])),
        )]);
        let v = V::A("{oa{sa{sv}}}".into(), vec![V::E(Box::new(V::O("/org/bluez/hci0/dev_A".into())), Box::new(inner))]);
        let m = Msg { kind: METHOD_RETURN, reply_serial: Some(3), body: vec![v.clone()], ..Default::default() };
        let (d, _) = Msg::decode(&m.encode(9)).unwrap().unwrap();
        assert_eq!(d.body[0], v);
        let props = d.body[0].get("/org/bluez/hci0/dev_A").unwrap().get("org.bluez.Device1").unwrap();
        assert_eq!(props.get("RSSI").and_then(V::int), Some(-60));
        assert_eq!(props.get("Paired").and_then(V::bool), Some(true));
    }
}
