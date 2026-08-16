#!/usr/bin/env python3
"""Check the fixtures under scripts/fixtures/grain/ against js/grain.js's format.

This cannot re-implement AES-256-GCM to prove js/grain.js encrypts correctly —
stdlib Python has no AES-GCM, and adding one would mean taking on a dependency
this repo's no-package-manager stance rules out. So it does the next best
thing, in three independent steps:

  1. Structural: parses stego.png's pixels and the 42-byte header itself,
     without trusting anything js/grain.js claims about its own format. This
     catches a header field, a bit order or a channel choice that drifted
     from what the shipped code actually does.
  2. Cross-check: hashlib.pbkdf2_hmac (Python's own PBKDF2-HMAC-SHA-256) is run
     against the same salt and iteration count the header carries, and held
     against scripts/fixtures/grain/derived_key.hex — a raw AES key exported
     from a real WebCrypto deriveKey() call during fixture generation (see the
     comment in gen_fixtures, kept out of the repo since it is a one-off
     script, not part of the shipped page). Two independent implementations of
     the same derivation agreeing is a real check, not a circular one.
  3. Regression pin: AES-GCM is deterministic given a fixed key, iv,
     additionalData and plaintext, so the ciphertext this script extracts from
     stego.png should equal the one committed at
     scripts/fixtures/grain/ciphertext.bin, generated the same way.

Step 3 is a regression pin, not a proof. A systematic bug present in both the
generation and this check — the wrong AAD, a byte order flip applied
consistently — would reproduce identically here and pass invisibly. What it
does catch is any future change to js/grain.js's header layout, bit order or
crypto parameters that isn't matched by regenerating the fixtures, which is
the drift this script exists to catch.

Stdlib only, by design. See ARCHITECTURE.md#continuous-integration.
"""

import hashlib
import struct
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "scripts" / "fixtures" / "grain"

HEADER_LEN = 42
MAGIC = b"GRN1"
VERSION = 1

failures = []


def fail(message):
    failures.append(message)


def paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    if pb <= pc:
        return b
    return c


def unfilter(raw, width, height, bpp):
    """Reverse PNG's per-scanline filtering. All five filter types, because
    a real browser encoder chooses one per row rather than using one type
    throughout."""
    stride = width * bpp
    out = bytearray(stride * height)
    pos = 0
    prev = bytearray(stride)
    for y in range(height):
        filt = raw[pos]
        pos += 1
        line = bytearray(raw[pos : pos + stride])
        pos += stride
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if filt == 0:
                pass
            elif filt == 1:
                line[i] = (line[i] + a) & 0xFF
            elif filt == 2:
                line[i] = (line[i] + b) & 0xFF
            elif filt == 3:
                line[i] = (line[i] + (a + b) // 2) & 0xFF
            elif filt == 4:
                line[i] = (line[i] + paeth(a, b, c)) & 0xFF
            else:
                raise ValueError(f"unknown PNG filter type {filt}")
        out[y * stride : (y + 1) * stride] = line
        prev = line
    return bytes(out)


def read_png_rgba(path):
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path.name}: not a PNG (bad signature)")
    pos = 8
    idat = b""
    width = height = bit_depth = color_type = None
    while pos < len(data):
        length = struct.unpack(">I", data[pos : pos + 4])[0]
        tag = data[pos + 4 : pos + 8]
        payload = data[pos + 8 : pos + 8 + length]
        if tag == b"IHDR":
            width, height, bit_depth, color_type = struct.unpack(">IIBB", payload[:10])
        elif tag == b"IDAT":
            idat += payload
        pos += 12 + length
    if width is None:
        raise ValueError(f"{path.name}: no IHDR chunk")
    if bit_depth != 8 or color_type != 6:
        raise ValueError(
            f"{path.name}: expected 8-bit RGBA (PNG color type 6) -- js/grain.js's "
            f"canvas round trip always produces this -- got bit depth {bit_depth}, "
            f"color type {color_type}"
        )
    raw = zlib.decompress(idat)
    return width, height, unfilter(raw, width, height, bpp=4)


def extract_bits(data, byte_count):
    """The same LSB read js/grain.js's extractBits() does: the low bit of
    each pixel's R, G, B channel, row-major, MSB-first within each byte."""
    out = bytearray(byte_count)
    total_bits = byte_count * 8
    bit_index = 0
    for p in range(0, len(data), 4):
        if bit_index >= total_bits:
            break
        for c in range(3):
            if bit_index >= total_bits:
                break
            bit = data[p + c] & 1
            out[bit_index >> 3] |= bit << (7 - (bit_index & 7))
            bit_index += 1
    return bytes(out)


def parse_header(header):
    if header[:4] != MAGIC:
        raise ValueError(f"bad magic {header[:4]!r}, expected {MAGIC!r}")
    version = header[4]
    if version != VERSION:
        raise ValueError(f"unexpected header version {version}, expected {VERSION}")
    (kdf_iter,) = struct.unpack(">I", header[6:10])
    salt = header[10:26]
    iv = header[26:38]
    (ct_len,) = struct.unpack(">I", header[38:42])
    return kdf_iter, salt, iv, ct_len


def main():
    stego_path = FIXTURES / "stego.png"
    carrier_path = FIXTURES / "carrier.png"
    if not stego_path.is_file() or not carrier_path.is_file():
        fail("scripts/fixtures/grain/carrier.png and stego.png must both exist")
        return report()

    # 1. Structural: independently parse the PNG and the 42-byte header.
    try:
        cw, ch, _ = read_png_rgba(carrier_path)
        sw, sh, sdata = read_png_rgba(stego_path)
    except ValueError as exc:
        fail(str(exc))
        return report()

    if (cw, ch) != (sw, sh):
        fail(f"carrier.png is {cw}x{ch} but stego.png is {sw}x{sh} -- they should be the same carrier")
        return report()

    header = extract_bits(sdata, HEADER_LEN)
    try:
        kdf_iter, salt, iv, ct_len = parse_header(header)
    except ValueError as exc:
        fail(f"stego.png header: {exc}")
        return report()

    capacity = (sw * sh * 3) // 8
    if HEADER_LEN + ct_len > capacity:
        fail(
            f"stego.png's header claims {ct_len} ciphertext bytes, but a "
            f"{sw}x{sh} image only has room for {capacity - HEADER_LEN}"
        )
        return report()

    # Alpha must read back fully opaque everywhere. js/grain.js forces this
    # before embedding, specifically so a later decode's premultiplication
    # cannot corrupt the low bit this page just wrote.
    if any(sdata[i] != 255 for i in range(3, len(sdata), 4)):
        fail("stego.png has a non-opaque pixel -- alpha should be forced to 255 before embedding")

    container = extract_bits(sdata, HEADER_LEN + ct_len)
    ciphertext = container[HEADER_LEN:]

    # 2. Cross-check against a real WebCrypto-derived key.
    passphrase = (FIXTURES / "passphrase.txt").read_text(encoding="utf-8").strip()
    want_key = (FIXTURES / "derived_key.hex").read_text(encoding="utf-8").strip()
    got_key = hashlib.pbkdf2_hmac("sha256", passphrase.encode("utf-8"), salt, kdf_iter, dklen=32).hex()
    if got_key != want_key:
        fail(
            "PBKDF2 cross-check failed: hashlib.pbkdf2_hmac(passphrase, salt, "
            "kdf_iter) does not match derived_key.hex, a key exported from a "
            "real WebCrypto deriveKey() call. Either the KDF parameters "
            "drifted from js/grain.js's deriveKey(), or the fixture is stale."
        )

    # 3. Regression pin against the committed ciphertext.
    want_ct = (FIXTURES / "ciphertext.bin").read_bytes()
    if ciphertext != want_ct:
        fail(
            f"stego.png's embedded ciphertext ({len(ciphertext)} bytes) does not "
            f"match the committed scripts/fixtures/grain/ciphertext.bin pin -- see "
            "this script's module docstring for what that pin can and cannot prove"
        )

    return report()


def report():
    if failures:
        print("Grain fixture check failed:\n")
        for message in failures:
            print(f"  {message}")
        return 1
    print(
        "Grain fixtures check out: stego.png's header and ciphertext parse "
        "correctly, hashlib.pbkdf2_hmac matches the WebCrypto-derived key, and "
        "the ciphertext matches its committed pin."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
