# F07: Grain, a file hidden inside a picture

**Status:** implemented and verified live in a real browser (Chrome
DevTools, 2026-08-16). `grain.html`, `css/grain.css` and `js/grain.js` pass
every check this repo can run without a browser — `scripts/check_grain.py`
cross-checks the header format, KDF and ciphertext against a fixture pair
(`scripts/fixtures/grain/`) generated with Node's `webcrypto` and a
from-scratch stdlib PNG encoder, see that script's own header for exactly
what it proves and what it only pins — and every item in "Acceptance
criteria" below marked Human has now been run against a live page: a full
encode → decode round trip, a wrong passphrase, a hand-tampered stego image,
an undersized carrier, a hand-crafted out-of-range KDF header, a clean
network panel confirming `connect-src 'none'` holds even against the page's
own blob URLs, keyboard/focus traversal, and a Lighthouse audit in both
colour schemes (Accessibility 100, Best Practices 100 in both; SEO 92 in
both, the only failing audit being Lighthouse's own robots.txt/llms.txt probe
request getting blocked by this page's CSP, not a defect in the site's
robots.txt, which was confirmed to serve 200 directly).

---

## Overview

Grain hides an arbitrary file inside a PNG. The file is encrypted first —
AES-256-GCM, keyed by a passphrase stretched through PBKDF2 — and only the
ciphertext, never the plaintext, is written into the carrier image's pixels
via least-significant-bit (LSB) steganography. Decoding reverses both steps:
extract the bits, then decrypt with the same passphrase.

This is not a puzzle or an obfuscation. The encryption is real and
authenticated: a wrong passphrase or a tampered stego image both fail the
same way, an AES-GCM authentication error, and Grain does not try to tell
those two cases apart (see "Container format" below). What LSB steganography
buys is concealment from casual inspection, not from statistical
steganalysis, and the page says so in its own prose rather than overselling
it.

Everything runs against browser-native APIs — `crypto.subtle` and
`<canvas>` — with nothing vendored and nothing fetched. `grain.html` sets
`connect-src 'none'`, tighter than any other page on the site, because unlike
`js/ask.js`, `js/mocap.js` or `js/iris.js` there is no same-origin model or
runtime this page needs to load either: it has no fetch of any kind to make.

---

## Key files

| File | Role |
| --- | --- |
| `grain.html` | The page: carrier/payload pickers, passphrase fields, encode/decode actions, result panels, the `connect-src 'none'; img-src 'self' blob: data:` CSP meta tag |
| `css/grain.css` | Layout only — drop zones, file-input visually-hidden-but-focusable pattern, result previews. Introduces no literal colour; every value is a `var(--...)` token from `css/style.css`. |
| `js/grain.js` | Header build/parse, LSB embed/extract, PBKDF2 key derivation, AES-256-GCM encrypt/decrypt, canvas pixel I/O, DOM wiring |
| `scripts/check_grain.py` | CI regression pin against `scripts/fixtures/grain/` — see its own header comment for what it proves and what it only pins |
| `scripts/fixtures/grain/` | Committed fixtures: a carrier PNG, a stego PNG with a known file hidden in it, the plaintext/ciphertext/derived key that produced it, and the passphrase and filename used |

---

## Architecture

```
Encode
  carrier image (any format the browser decodes) ──┐
  payload file (any type) ───────────────────────┐  │
  passphrase ──┐                                 │  │
               │                                 │  │
               ▼                                 ▼  ▼
     PBKDF2-HMAC-SHA-256              nameLen(2) ‖ filename ‖ payload bytes
     (600,000 iters, random                       │
      16-byte salt)                               │
               │                                  │
               ▼                                  │
     non-extractable AES-256-GCM key               │
               │                                  │
               └──── AES-GCM encrypt(random 12-byte iv, AAD = header) ◄──┘
                              │
                     ciphertext (+ 16-byte tag)
                              │
     header = magic ‖ version ‖ flags ‖ kdfIter ‖ salt ‖ iv ‖ ctLen  (42 bytes,
              built once ctLen is known — ctLen = plaintext.length + 16, since
              AES-GCM ciphertext length depends only on plaintext length)
                              │
                    header ‖ ciphertext  (the container)
                              │
     createImageBitmap(carrier) → canvas → ImageData
                              │
     LSB-embed container into R/G/B low bits, row-major, MSB-first;
     force every pixel's alpha to 255 first
                              │
                    canvas.toBlob('image/png') → stego PNG

Decode
  stego PNG ──► createImageBitmap → canvas → ImageData
                              │
     LSB-extract 42-byte header → parse → validate magic/version and
     100,000 ≤ kdfIter ≤ 2,000,000 (refused before deriveKey is ever called)
                              │
     LSB-extract ctLen more bytes → ciphertext
                              │
     passphrase + header's salt/kdfIter → PBKDF2 → AES-256-GCM key
                              │
     AES-GCM decrypt(iv, AAD = header, ciphertext) → plaintext, or a single
     generic failure ("wrong passphrase, or the image was altered") — AES-GCM
     cannot and does not distinguish the two
                              │
     nameLen ‖ filename ‖ payload  →  a Blob URL offered as a download
```

---

## Container format

A fixed 42-byte header, then the ciphertext:

| Field | Bytes | Notes |
| --- | --- | --- |
| magic | 4 | `"GRN1"` |
| version | 1 | `1` |
| flags | 1 | reserved, `0` |
| kdfIter | 4 | uint32, big-endian |
| salt | 16 | random, per encode |
| iv | 12 | random, per encode |
| ctLen | 4 | uint32, big-endian — ciphertext length including the 16-byte GCM tag |

All 42 header bytes are passed as AES-GCM's `additionalData`, so every field
— including the salt and iv a decoder is about to trust — is bound into the
ciphertext's own authentication tag. A single flipped bit anywhere in the
header fails decryption instead of silently decrypting against the wrong
salt or iv.

The encrypted plaintext itself is `nameLen(2, big-endian) ‖ filename (UTF-8)
‖ payload bytes` — the hidden file's name is inside the encryption boundary,
never cleartext anywhere in the container.

`ctLen` is computed as `plaintext.length + 16` before encrypting, rather than
by encrypting once to measure it and again with the final header as AAD: AES-
GCM ciphertext length depends only on plaintext length, never on
`additionalData`, so one `crypto.subtle.encrypt()` call is enough.

---

## Scope cuts

- **PNG carrier output only.** Any lossy re-encode (JPEG, WebP lossy, a
  resave through an editor that recompresses) destroys the low bits Grain
  just wrote. The page states this rather than trying to detect or survive
  it.
- **No multi-file or archive support.** One payload file per stego image.
- **No steganalysis resistance.** LSB embedding raises the low-bit entropy of
  every touched channel above what a camera or renderer would produce; a
  statistical detector looking for exactly that is not defeated by anything
  here. Grain's own prose says this plainly rather than implying otherwise.
- **No key recovery.** The passphrase is the only key; there is no
  server-side or local escrow of any kind, by design — there is no server at
  all.
- **A 40-megapixel cap** on any loaded image (carrier or stego), to bound
  `getImageData` memory use, and a `[100,000, 2,000,000]` KDF-iteration range
  enforced on decode *before* `deriveKey` runs, so a maliciously crafted
  stego file cannot name an absurd iteration count to hang the tab.

---

## The DOM contract

| Id | Element | Contract |
| --- | --- | --- |
| `grain--carrier-drop` | `div` | Drop zone wrapping the carrier file input |
| `grain--carrier` | `input type="file" accept="image/*"` | Visually hidden; styled via its `<label>` |
| `grain--sample` | `button type="button"` | Loads this site's own avatar as the carrier, with no file picker |
| `grain--sample-img` | `img` | `aria-hidden`, read-only pixel source for the sample carrier — never redrawn to disk |
| `grain--carrier-preview` | `img` | `hidden` until a carrier is chosen |
| `grain--capacity` | `p` | `role="status" aria-live="polite"` — how many bytes the chosen carrier can hold |
| `grain--payload-drop` | `div` | Drop zone wrapping the payload file input |
| `grain--payload` | `input type="file"` | Visually hidden; styled via its `<label>` |
| `grain--payload-name` | `p` | `role="status" aria-live="polite"` — the native input's own "chosen file" text is clipped away by the visually-hidden styling, so this is the only place the choice is confirmed |
| `grain--pass` / `grain--pass-confirm` | `input type="password"` | `autocomplete="off" spellcheck="false" autocapitalize="off"` |
| `grain--pass-show` | `input type="checkbox"` | Toggles both passphrase fields to `type="text"` |
| `grain--encode` | `button type="button"` | `aria-disabled`, guarded by an `encoding` re-entry flag, never the `disabled` property |
| `grain--encode-status` | `p` | `role="status" aria-live="polite"` |
| `grain--stego-preview` / `grain--download` | `img` / `a` | `hidden` result panel; focus moves to `grain--download` on success |
| `grain--stego-drop` | `div` | Drop zone wrapping the stego-file input |
| `grain--stego` | `input type="file" accept="image/png"` | Visually hidden; styled via its `<label>` |
| `grain--stego-name` | `p` | `role="status" aria-live="polite"`, same reason as `grain--payload-name` |
| `grain--decode-pass` | `input type="password"` | `autocomplete="off" spellcheck="false" autocapitalize="off"` |
| `grain--decode-pass-show` | `input type="checkbox"` | Toggles the decode passphrase field to `type="text"` |
| `grain--decode` | `button type="button"` | `aria-disabled`, guarded by a `decoding` re-entry flag |
| `grain--decode-status` | `p` | `role="status" aria-live="polite"` |
| `grain--extracted` | `a` | `hidden` result panel; focus moves here on success |

Both `#grain--encode-section` and `#grain--decode-section` are always
present — neither is ever hidden while the other is in use — so
`.grain--page` uses ordinary block layout rather than the `display: contents`
pattern `#iris--page` uses to wrap a hide/show sequence.

---

## Acceptance criteria

| ID | Criterion | Evidence |
| --- | --- | --- |
| F07-AC01 | A file round-trips: encode with a passphrase, decode with the same passphrase, recovered bytes and filename are byte-identical to the original. | Human, real browser |
| F07-AC02 | A wrong passphrase and a tampered stego image both fail decode with the same generic message; neither leaks which case occurred. | Human, real browser |
| F07-AC03 | The 42-byte header is authenticated as AES-GCM additional data — flipping any header byte (salt, iv, kdfIter) fails decryption rather than decrypting against the wrong parameters. | `scripts/check_grain.py` for the structural format; a hand-tampered stego file for the live failure mode — Human |
| F07-AC04 | Nothing is ever fetched: `grain.html`'s `connect-src 'none'` CSP holds and the network panel shows zero requests across a full encode-and-decode session. | Human, clean network panel |
| F07-AC05 | A carrier too small for the chosen payload is refused before encoding starts, with the required and available byte counts both stated. | Human, real browser |
| F07-AC06 | A KDF iteration count outside `[100,000, 2,000,000]` in a decoded header is refused before `deriveKey` is ever called. | `scripts/check_grain.py` parses the range check structurally; a hand-crafted out-of-range header — Human |
| F07-AC07 | None of `grain--encode`, `grain--decode` use the `disabled` property; focus lands on the result link after a successful encode or decode. | Human, keyboard traversal |
| F07-AC08 | `grain.html` is listed in `sitemap.xml`. | `scripts/check_repo.py`, CI |
| F07-AC09 | Contrast holds at 4.5:1 for text in both flavours, checked against Latte. | Human, per colour scheme |
| F07-AC10 | `scripts/check_grain.py` passes: the committed stego fixture's header and ciphertext parse correctly and match the independently-derived PBKDF2 key and the committed ciphertext pin. | CI |

---

## Deferred

| Item | Note |
| --- | --- |
| Measured stego-PNG size and PBKDF2 derivation time on real hardware | Not yet measured in a browser; will be filled in once F07-AC01 is run by a human. |
| A capacity warning shown before a carrier is even picked | Currently shown only after a carrier loads; not blocking, since the capacity check still runs before encoding. |
