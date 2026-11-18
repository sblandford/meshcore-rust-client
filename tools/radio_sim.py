#!/usr/bin/env python3
"""Simulated MeshCore companion radio on a pty, for testing the desktop client."""
import os, struct, time, select, sys, tty

master, slave = os.openpty()
tty.setraw(slave)
path = os.ttyname(slave)
print('pty', path, flush=True)

ME = bytes(range(0xA0, 0xC0))
def key(n): return bytes([n] * 32)
CONTACTS = [
    (key(0x11), 1, 1, 0xFF, b'', b'Alice'),
    (key(0x22), 2, 0, 2, bytes([0x5A, 0x7C]), b'Hilltop Repeater'),
    (key(0x33), 3, 0, 1, bytes([0x5A]), b'Village Room'),
    (key(0x44), 1, 0, 0, b'', b'Bob \xf0\x9f\x9a\xb2'),
]
CHANNELS = {0: (b'Public', bytes.fromhex('8b3387e9c5cdea6ac9e5edbaa115cd72')), 1: (b'#test', bytes(range(16)))}
now = int(time.time())
queue = []
def chan_msg(idx, text, snr=-6, hops=2, ts=None):
    return bytes([17, (snr * 4) & 0xFF, 0, 0, idx, hops, 0]) + struct.pack('<I', ts or now - 300) + text
def dm(pk, text, txt=0, extra=b'', ts=None, hops=0xFF):
    return bytes([16, 28, 0, 0]) + pk[:6] + bytes([hops, txt]) + struct.pack('<I', ts or now - 120) + extra + text
queue += [chan_msg(0, b'Gary: Good morning mesh!', ts=now - 3600), chan_msg(0, b'Dave: Anyone on the hill today?', hops=4),
          dm(key(0x11), b'Hi there, are you receiving me?'), chan_msg(1, b'Tester: ping 1')]

def contact_frame(code, c):
    pk, typ, flags, plen, path, name = c
    return bytes([code]) + pk + bytes([typ, flags, plen]) + path.ljust(64, b'\0') + name.ljust(32, b'\0') + \
        struct.pack('<IiiI', now - 600, 51500000, -120000, now)

def send(frame):
    os.write(master, b'>' + struct.pack('<H', len(frame)) + frame)

buf = b''
pending = []  # (time, frame)
def later(dt, f): pending.append((time.time() + dt, f))

def handle(f):
    c = f[0]
    if c == 22:
        send(bytes([13, 13, 175, 8]) + struct.pack('<I', 123456) + b'1 Nov 2026'.ljust(12, b'\0') +
             b'Simulated T-Echo'.ljust(40, b'\0') + b'v1.18.0'.ljust(20, b'\0') + b'\0\0')
    elif c == 1:
        send(bytes([5, 1, 20, 22]) + ME + struct.pack('<ii', 51507400, -127800) + bytes([1, 1, 0, 0]) +
             struct.pack('<II', 869525, 250000) + bytes([11, 5]) + b'Desk Node')
    elif c == 4:
        send(bytes([2]) + struct.pack('<I', len(CONTACTS)))
        for ct in CONTACTS: send(contact_frame(3, ct))
        send(bytes([4]) + struct.pack('<I', now))
    elif c == 30:
        ct = next((x for x in CONTACTS if x[0] == f[1:33]), None)
        send(contact_frame(3, ct) if ct else bytes([1, 2]))
    elif c == 31:
        i = f[1]
        n, s = CHANNELS.get(i, (b'', bytes(16)))
        send(bytes([18, i]) + n.ljust(32, b'\0') + s) if i < 8 else send(bytes([1, 2]))
    elif c == 32:
        CHANNELS[f[1]] = (f[2:34].rstrip(b'\0'), f[34:50]); send(bytes([0]))
    elif c == 10:
        send(queue.pop(0) if queue else bytes([10]))
    elif c == 20:
        send(bytes([12]) + struct.pack('<HII', 4012, 96, 2048))
    elif c == 2:
        send(bytes([6, 0]) + struct.pack('<II', 0x11223344 + f[2], 2500))
        if f[2] >= 1 or f[7:13] != key(0x44)[:6]:  # Bob only answers on the retry
            later(1.5, bytes([0x82]) + struct.pack('<II', 0x11223344 + f[2], 1432))
    elif c == 3:
        send(bytes([0]))
    elif c == 26:
        send(bytes([6, 1]) + f[1:5] + struct.pack('<I', 4000))
        ok = f[33:] == b'secret'
        later(1.0, (bytes([0x85, 1]) if ok else bytes([0x86, 0])) + f[1:7])
    elif c == 27:
        send(bytes([6, 0, 1, 2, 3, 4]) + struct.pack('<I', 3000))
        later(1.0, bytes([0x87, 0]) + f[1:7] + struct.pack('<HHhhIIIIIIIIHhHHII', 4105, 0, -112, -87, 5321, 1210, 3600, 86400 * 3 + 7200, 900, 310, 4000, 1300, 2, -22, 12, 340, 7200, 3))
    elif c == 39:
        send(bytes([6, 0, 5, 6, 7, 8]) + struct.pack('<I', 3000))
        later(1.0, bytes([0x8B, 0]) + f[4:10] + bytes([1, 116, 0x01, 0x9A, 1, 103, 0x00, 0xD2]))
    elif c == 36:
        tag = f[1:5]; path = f[10:]
        send(bytes([6, 0]) + tag + struct.pack('<I', 3000))
        snrs = bytes([(x * 4) & 0xFF for x in [9, 7, 6][:len(path)]])
        later(1.2, bytes([0x89, 0, len(path), f[9]]) + tag + bytes(4) + path + snrs + bytes([40]))
    elif c == 7:
        send(bytes([0]))
    else:
        send(bytes([0]))

start = time.time()
tickled = False
while True:
    r, _, _ = select.select([master], [], [], 0.1)
    if r:
        try:
            buf += os.read(master, 1024)
        except OSError:
            time.sleep(0.2); continue
        while True:
            i = buf.find(b'<')
            if i < 0: buf = b''; break
            buf = buf[i:]
            if len(buf) < 3: break
            n = struct.unpack('<H', buf[1:3])[0]
            if len(buf) < 3 + n: break
            f = buf[3:3 + n]; buf = buf[3 + n:]
            print('cmd', f[0], f.hex()[:40], flush=True)
            handle(f)
    t = time.time()
    for p in [p for p in pending if p[0] <= t]:
        pending.remove(p); send(p[1])
    if not tickled and t - start > 12:
        tickled = True
        queue.append(dm(key(0x33), b'Welcome to the village room', txt=2, extra=key(0x44)[:4], ts=now - 30, hops=1))
        send(bytes([0x83]))
        send(bytes([0x88, 30, (-90) & 0xFF]) + bytes([0x15, 2, 0x5A, 0x7C]) + bytes(20))
