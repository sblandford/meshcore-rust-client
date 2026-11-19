//! Local persistence: per-device message history (append-only, tab-separated text)
//! and a tiny key=value settings file.

use crate::app::{ChatKey, Message, Status};
use crate::util::{hex, unhex};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n").replace('\r', "")
}

fn unesc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('t') => o.push('\t'),
                Some('n') => o.push('\n'),
                Some(c) => o.push(c),
                None => {}
            }
        } else {
            o.push(c);
        }
    }
    o
}

pub fn key_str(k: &ChatKey) -> String {
    match k {
        ChatKey::Channel(s) => format!("c{}", hex(s)),
        ChatKey::Contact(p) => format!("k{}", hex(p)),
    }
}

fn parse_key(s: &str) -> Option<ChatKey> {
    let b = unhex(s.get(1..)?)?;
    match s.as_bytes()[0] {
        b'c' => Some(ChatKey::Channel(b.try_into().ok()?)),
        b'k' => Some(ChatKey::Contact(b.try_into().ok()?)),
        _ => None,
    }
}

fn status_code(s: Status) -> String {
    match s {
        Status::Received => "r".into(),
        Status::Pending | Status::Sent => "s".into(),
        Status::Delivered(rtt) => format!("d{rtt}"),
        Status::Failed => "f".into(),
    }
}

fn parse_status(s: &str) -> Status {
    match s.as_bytes().first() {
        Some(b'r') => Status::Received,
        Some(b'd') => Status::Delivered(s[1..].parse().unwrap_or(0)),
        Some(b'f') => Status::Failed,
        _ => Status::Sent,
    }
}

pub struct Store {
    file: Option<File>,
}

pub type History = HashMap<ChatKey, Vec<Message>>;

impl Store {
    /// Open (creating if needed) the history for the radio with the given public key.
    pub fn open(pubkey: &[u8; 32]) -> (Store, History, u64) {
        Self::open_path(crate::util::data_dir().join(format!("history-{}.log", hex(&pubkey[..8]))))
    }

    fn open_path(path: PathBuf) -> (Store, History, u64) {
        let mut hist: History = HashMap::new();
        let mut max_id = 0;
        let mut index: HashMap<u64, ChatKey> = HashMap::new();
        if let Ok(f) = File::open(&path) {
            for line in BufReader::new(f).lines().map_while(Result::ok) {
                let p: Vec<&str> = line.split('\t').collect();
                match p.first().copied() {
                    Some("M") if p.len() >= 10 => {
                        let (Some(key), Ok(id)) = (parse_key(p[2]), p[1].parse::<u64>()) else { continue };
                        let m = Message {
                            id,
                            out: p[3] == "1",
                            ts: p[4].parse().unwrap_or(0),
                            recv: p[5].parse().unwrap_or(0),
                            status: parse_status(p[6]),
                            txt_type: p[7].parse().unwrap_or(0),
                            author: unesc(p[8]),
                            text: unesc(p[9]),
                            snr: p.get(10).and_then(|s| s.parse().ok()),
                            path_len: p.get(11).and_then(|s| s.parse().ok()).unwrap_or(0xFF),
                            ack: 0,
                            attempt: 0,
                            deadline: None,
                            repeats: Vec::new(),
                        };
                        max_id = max_id.max(id);
                        index.insert(id, key);
                        hist.entry(key).or_default().push(m);
                    }
                    Some("S") if p.len() >= 3 => {
                        let Ok(id) = p[1].parse::<u64>() else { continue };
                        if let Some(m) = index.get(&id).and_then(|k| hist.get_mut(k)).and_then(|v| v.iter_mut().rev().find(|m| m.id == id)) {
                            m.status = parse_status(p[2]);
                        }
                    }
                    Some("R") if p.len() >= 4 => {
                        let Ok(id) = p[1].parse::<u64>() else { continue };
                        if let Some(m) = index.get(&id).and_then(|k| hist.get_mut(k)).and_then(|v| v.iter_mut().rev().find(|m| m.id == id)) {
                            m.repeats.push((unhex(p[2]).unwrap_or_default(), p[3].parse().unwrap_or(0.0)));
                        }
                    }
                    Some("D") if p.len() >= 2 => {
                        let Ok(id) = p[1].parse::<u64>() else { continue };
                        if let Some(v) = index.get(&id).and_then(|k| hist.get_mut(k)) {
                            v.retain(|m| m.id != id);
                        }
                    }
                    Some("C") if p.len() >= 2 => {
                        if let Some(k) = parse_key(p[1]) {
                            hist.remove(&k);
                        }
                    }
                    _ => {}
                }
            }
        }
        // Anything still awaiting an ACK when the app last closed is unknown/failed.
        for (k, v) in hist.iter_mut() {
            for m in v.iter_mut() {
                if m.out && m.status == Status::Sent && m.txt_type == 0 && matches!(k, ChatKey::Contact(_)) {
                    m.status = Status::Failed;
                }
            }
        }
        let file = OpenOptions::new().create(true).append(true).open(&path).ok();
        (Store { file }, hist, max_id + 1)
    }

    fn write(&mut self, line: String) {
        if let Some(f) = &mut self.file {
            let _ = f.write_all(line.as_bytes());
        }
    }

    pub fn add(&mut self, key: &ChatKey, m: &Message) {
        let snr = m.snr.map(|s| s.to_string()).unwrap_or_default();
        self.write(format!(
            "M\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            m.id,
            key_str(key),
            m.out as u8,
            m.ts,
            m.recv,
            status_code(m.status),
            m.txt_type,
            esc(&m.author),
            esc(&m.text),
            snr,
            m.path_len
        ));
    }

    /// Record that a relay was heard re-transmitting an outgoing message.
    pub fn add_repeat(&mut self, id: u64, relay: &[u8], snr: f32) {
        self.write(format!("R\t{id}\t{}\t{snr}\n", hex(relay)));
    }

    /// Record that one message was deleted.
    pub fn delete(&mut self, id: u64) {
        self.write(format!("D\t{id}\n"));
    }

    /// Record that a conversation's history was cleared.
    pub fn clear(&mut self, key: &ChatKey) {
        self.write(format!("C\t{}\n", key_str(key)));
    }

    pub fn set_status(&mut self, id: u64, s: Status) {
        self.write(format!("S\t{id}\t{}\n", status_code(s)));
    }
}

// ---------------------------------------------------------------- settings

pub struct Settings {
    pub map: Vec<(String, String)>,
}

impl Settings {
    fn path() -> PathBuf {
        crate::util::data_dir().join("settings.txt")
    }
    pub fn load() -> Settings {
        let map = std::fs::read_to_string(Self::path())
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once('=').map(|(k, v)| (k.to_string(), unesc(v))))
            .collect();
        Settings { map }
    }
    pub fn get(&self, k: &str) -> Option<&str> {
        self.map.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
    }
    pub fn set(&mut self, k: &str, v: &str) {
        match self.map.iter_mut().find(|(a, _)| a == k) {
            Some(e) => e.1 = v.to_string(),
            None => self.map.push((k.into(), v.into())),
        }
        let s: String = self.map.iter().map(|(k, v)| format!("{k}={}\n", esc(v))).collect();
        let _ = std::fs::write(Self::path(), s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_records_survive_reload() {
        let path = std::env::temp_dir().join(format!("mc-store-test-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let msg = |id: u64, text: &str| Message {
            id,
            out: true,
            ts: 1,
            recv: 2,
            author: "me".into(),
            text: text.into(),
            status: Status::Sent,
            txt_type: 0,
            snr: None,
            path_len: 0xFF,
            ack: 0,
            attempt: 0,
            deadline: None,
            repeats: Vec::new(),
        };
        let (a, b) = (ChatKey::Contact([1; 32]), ChatKey::Channel([2; 16]));
        {
            let (mut st, _, _) = Store::open_path(path.clone());
            st.add(&a, &msg(1, "keep\twith tab"));
            st.add(&a, &msg(2, "delete me"));
            st.add(&a, &msg(3, "delivered"));
            st.set_status(3, Status::Delivered(900));
            st.delete(2);
            st.add(&b, &msg(4, "cleared"));
            st.clear(&b);
            st.add(&b, &msg(5, "after clear"));
        }
        let (_, h, next) = Store::open_path(path.clone());
        let _ = std::fs::remove_file(&path);
        let ta: Vec<_> = h[&a].iter().map(|m| (m.id, m.text.as_str(), m.status)).collect();
        // unacknowledged DM from a previous session is shown as failed
        assert_eq!(ta, vec![(1, "keep\twith tab", Status::Failed), (3, "delivered", Status::Delivered(900))]);
        assert_eq!(h[&b].iter().map(|m| m.id).collect::<Vec<_>>(), vec![5]);
        assert_eq!(next, 6);
    }

    #[test]
    fn escaping() {
        let s = "a\tb\nc\\d";
        assert_eq!(unesc(&esc(s)), s);
        assert!(!esc(s).contains('\t'));
    }
}
