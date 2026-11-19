//! Small self-contained helpers: hex, base64, SHA-256, time formatting, paths.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn hex(b: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for &x in b {
        s.push(H[(x >> 4) as usize] as char);
        s.push(H[(x & 15) as usize] as char);
    }
    s
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    let v = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    };
    s.as_bytes().chunks(2).map(|p| Some(v(p[0])? << 4 | v(p[1])?)).collect()
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.trim().bytes() {
        if c == b'=' {
            break;
        }
        let v = match c {
            b'-' => 62,
            b'_' => 63,
            _ => B64.iter().position(|&x| x == c)? as u32,
        };
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
pub fn b64_encode(b: &[u8]) -> String {
    let mut s = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// SHA-256 (FIPS 180-4). Used to derive hashtag-channel secrets.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(v[i]);
        }
    }
    let mut out = [0u8; 32];
    for i in 0..8 {
        out[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}

/// HMAC-SHA256 (RFC 2104) for keys up to 64 bytes.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&sha256(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner: Vec<u8> = k.iter().map(|b| b ^ 0x36).collect();
    inner.extend_from_slice(data);
    let mut outer: Vec<u8> = k.iter().map(|b| b ^ 0x5c).collect();
    outer.extend_from_slice(&sha256(&inner));
    sha256(&outer)
}

// ---------------------------------------------------------------- AES-128 (FIPS-197)

fn gmul(mut a: u8, mut b: u8) -> u8 {
    let mut p = 0;
    while b != 0 {
        if b & 1 != 0 {
            p ^= a;
        }
        let hi = a & 0x80;
        a <<= 1;
        if hi != 0 {
            a ^= 0x1b;
        }
        b >>= 1;
    }
    p
}

/// (S-box, inverse S-box), derived from GF(2^8) inversion plus the affine transform.
fn sboxes() -> &'static ([u8; 256], [u8; 256]) {
    static T: std::sync::OnceLock<([u8; 256], [u8; 256])> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut s = [0u8; 256];
        let mut inv = [0u8; 256];
        for x in 0..256usize {
            let i = if x == 0 { 0 } else { (1..=255u8).find(|&y| gmul(x as u8, y) == 1).unwrap() };
            let v = i ^ i.rotate_left(1) ^ i.rotate_left(2) ^ i.rotate_left(3) ^ i.rotate_left(4) ^ 0x63;
            s[x] = v;
            inv[v as usize] = x as u8;
        }
        (s, inv)
    })
}

fn aes128_round_keys(key: &[u8; 16]) -> [[u8; 16]; 11] {
    let (sb, _) = sboxes();
    let mut w = [[0u8; 4]; 44];
    for i in 0..4 {
        w[i].copy_from_slice(&key[i * 4..i * 4 + 4]);
    }
    let mut rcon = 1u8;
    for i in 4..44 {
        let mut t = w[i - 1];
        if i % 4 == 0 {
            t = [sb[t[1] as usize] ^ rcon, sb[t[2] as usize], sb[t[3] as usize], sb[t[0] as usize]];
            rcon = gmul(rcon, 2);
        }
        for j in 0..4 {
            w[i][j] = w[i - 4][j] ^ t[j];
        }
    }
    let mut rk = [[0u8; 16]; 11];
    for (r, k) in rk.iter_mut().enumerate() {
        for c in 0..4 {
            k[c * 4..c * 4 + 4].copy_from_slice(&w[r * 4 + c]);
        }
    }
    rk
}

#[cfg(test)]
pub fn aes128_encrypt_block(key: &[u8; 16], block: &[u8; 16]) -> [u8; 16] {
    let (sb, _) = sboxes();
    let rk = aes128_round_keys(key);
    let mut s = *block;
    for i in 0..16 {
        s[i] ^= rk[0][i];
    }
    for round in 1..=10 {
        let mut t = [0u8; 16];
        for c in 0..4 {
            for r in 0..4 {
                t[r + 4 * c] = sb[s[r + 4 * ((c + r) % 4)] as usize]; // SubBytes + ShiftRows
            }
        }
        if round < 10 {
            for c in 0..4 {
                let a = [t[4 * c], t[4 * c + 1], t[4 * c + 2], t[4 * c + 3]];
                t[4 * c] = gmul(a[0], 2) ^ gmul(a[1], 3) ^ a[2] ^ a[3];
                t[4 * c + 1] = a[0] ^ gmul(a[1], 2) ^ gmul(a[2], 3) ^ a[3];
                t[4 * c + 2] = a[0] ^ a[1] ^ gmul(a[2], 2) ^ gmul(a[3], 3);
                t[4 * c + 3] = gmul(a[0], 3) ^ a[1] ^ a[2] ^ gmul(a[3], 2);
            }
        }
        for i in 0..16 {
            s[i] = t[i] ^ rk[round][i];
        }
    }
    s
}

fn aes128_decrypt_block(rk: &[[u8; 16]; 11], block: &[u8]) -> [u8; 16] {
    let (_, isb) = sboxes();
    let mut s = [0u8; 16];
    for i in 0..16 {
        s[i] = block[i] ^ rk[10][i];
    }
    for round in (0..10).rev() {
        let mut t = [0u8; 16];
        for c in 0..4 {
            for r in 0..4 {
                t[r + 4 * ((c + r) % 4)] = isb[s[r + 4 * c] as usize]; // InvShiftRows + InvSubBytes
            }
        }
        for i in 0..16 {
            t[i] ^= rk[round][i];
        }
        if round > 0 {
            for c in 0..4 {
                let a = [t[4 * c], t[4 * c + 1], t[4 * c + 2], t[4 * c + 3]];
                t[4 * c] = gmul(a[0], 14) ^ gmul(a[1], 11) ^ gmul(a[2], 13) ^ gmul(a[3], 9);
                t[4 * c + 1] = gmul(a[0], 9) ^ gmul(a[1], 14) ^ gmul(a[2], 11) ^ gmul(a[3], 13);
                t[4 * c + 2] = gmul(a[0], 13) ^ gmul(a[1], 9) ^ gmul(a[2], 14) ^ gmul(a[3], 11);
                t[4 * c + 3] = gmul(a[0], 11) ^ gmul(a[1], 13) ^ gmul(a[2], 9) ^ gmul(a[3], 14);
            }
        }
        s = t;
    }
    s
}

/// AES-128 in ECB mode, as MeshCore uses for packet payloads (input is whole blocks).
pub fn aes128_ecb_decrypt(key: &[u8; 16], data: &[u8]) -> Vec<u8> {
    let rk = aes128_round_keys(key);
    data.chunks_exact(16).flat_map(|b| aes128_decrypt_block(&rk, b)).collect()
}

/// Secret for a public "#hashtag" channel: first 16 bytes of SHA-256 of the name (including '#').
pub fn hashtag_secret(name: &str) -> [u8; 16] {
    sha256(name.as_bytes())[..16].try_into().unwrap()
}

/// Simple xorshift PRNG seeded from the clock (used for trace tags only).
pub fn rand_u32() -> u32 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static S: AtomicU64 = AtomicU64::new(0);
    let mut x = S.load(Ordering::Relaxed);
    if x == 0 {
        x = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64 | 1;
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    S.store(x, Ordering::Relaxed);
    (x >> 16) as u32
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ---------------------------------------------------------------- time formatting

/// Offset of local time from UTC in seconds, for the given UTC instant.
#[cfg(unix)]
fn utc_offset(t: u64) -> i64 {
    unsafe {
        let tt = t as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&tt, &mut tm).is_null() {
            return 0;
        }
        tm.tm_gmtoff as i64
    }
}

#[cfg(windows)]
fn utc_offset(t: u64) -> i64 {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::Storage::FileSystem::FileTimeToLocalFileTime;
    let ft = (t + 11_644_473_600) * 10_000_000;
    let utc = FILETIME { dwLowDateTime: ft as u32, dwHighDateTime: (ft >> 32) as u32 };
    let mut local = FILETIME::default();
    unsafe {
        if FileTimeToLocalFileTime(&utc, &mut local).is_err() {
            return 0;
        }
    }
    let l = (local.dwHighDateTime as u64) << 32 | local.dwLowDateTime as u64;
    (l as i64 - ft as i64) / 10_000_000
}

/// Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + (m <= 2) as i64, m, d)
}

pub struct Local {
    pub y: i64,
    pub mo: u32,
    pub d: u32,
    pub h: u32,
    pub mi: u32,
    pub s: u32,
}

pub fn local(t: u64) -> Local {
    let lt = t as i64 + utc_offset(t);
    let (y, mo, d) = civil(lt.div_euclid(86_400));
    let sod = lt.rem_euclid(86_400) as u32;
    Local { y, mo, d, h: sod / 3600, mi: sod / 60 % 60, s: sod % 60 }
}

/// "14:05" for today, otherwise "2026-11-17 14:05".
pub fn fmt_time(t: u64) -> String {
    if t == 0 {
        return "never".into();
    }
    let l = local(t);
    let n = local(now());
    if (l.y, l.mo, l.d) == (n.y, n.mo, n.d) {
        format!("{:02}:{:02}", l.h, l.mi)
    } else {
        format!("{}-{:02}-{:02} {:02}:{:02}", l.y, l.mo, l.d, l.h, l.mi)
    }
}

/// Full local date and time, e.g. "2026-11-19 09:41:07".
pub fn fmt_full(t: u64) -> String {
    let l = local(t);
    format!("{}-{:02}-{:02} {:02}:{:02}:{:02}", l.y, l.mo, l.d, l.h, l.mi, l.s)
}

pub fn fmt_clock(t: u64) -> String {
    let l = local(t);
    format!("{:02}:{:02}:{:02}", l.h, l.mi, l.s)
}

pub fn ago(t: u64) -> String {
    if t == 0 {
        return "never".into();
    }
    let n = now();
    if t > n + 60 {
        return fmt_time(t);
    }
    let d = n.saturating_sub(t);
    match d {
        0..=59 => "just now".into(),
        60..=3599 => format!("{}m ago", d / 60),
        3600..=86_399 => format!("{}h ago", d / 3600),
        _ => format!("{}d ago", d / 86_400),
    }
}

pub fn duration(secs: u32) -> String {
    let (d, h, m, s) = (secs / 86_400, secs / 3600 % 24, secs / 60 % 60, secs % 60);
    if d > 0 {
        format!("{d}d {h}h {m}m")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m {s}s")
    }
}

// ---------------------------------------------------------------- data directory

pub fn data_dir() -> std::path::PathBuf {
    use std::env::var_os;
    use std::path::PathBuf;
    let base = if cfg!(windows) {
        var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    let dir = base.unwrap_or_else(|| PathBuf::from(".")).join("meshcore-client");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha() {
        assert_eq!(hex(&sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(hex(&sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        let long = vec![b'a'; 1000];
        assert_eq!(hex(&sha256(&long)), "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3");
    }

    #[test]
    fn aes_and_hmac_vectors() {
        // FIPS-197 appendix C.1
        let key: [u8; 16] = unhex("000102030405060708090a0b0c0d0e0f").unwrap().try_into().unwrap();
        let pt: [u8; 16] = unhex("00112233445566778899aabbccddeeff").unwrap().try_into().unwrap();
        let ct = aes128_encrypt_block(&key, &pt);
        assert_eq!(hex(&ct), "69c4e0d86a7b0430d8cdb78070b4c55a");
        assert_eq!(aes128_ecb_decrypt(&key, &ct), pt.to_vec());
        // RFC 4231 test case 2
        assert_eq!(hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    }

    #[test]
    fn b64() {
        let k = b64_decode("izOH6cXN6mrJ5e26oRXNcg==").unwrap();
        assert_eq!(hex(&k), "8b3387e9c5cdea6ac9e5edbaa115cd72");
        assert_eq!(b64_encode(&k), "izOH6cXN6mrJ5e26oRXNcg==");
        assert_eq!(b64_encode(b"ab"), "YWI=");
    }

    #[test]
    fn dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_000), (2024, 10, 4));
        assert_eq!(civil(-1), (1969, 12, 31));
    }
}
