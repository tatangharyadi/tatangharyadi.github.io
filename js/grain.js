// Grain: hide an encrypted file inside a PNG's pixels.
//
// Two independent techniques, stacked. AES-256-GCM behind a passphrase does
// the actual protecting; least-significant-bit steganography just gives the
// ciphertext somewhere unremarkable to sit. Losing either half loses the
// file: the passphrase is the only key (no recovery), and re-saving the
// stego PNG through anything that recompresses pixels destroys the hidden
// bits before decryption ever runs.
//
// Nothing here calls fetch, XHR or WebSocket — this page's
// Content-Security-Policy (connect-src 'none') makes that a guarantee the
// browser enforces, not just a claim this file makes about itself.

const MAGIC = new Uint8Array([0x47, 0x52, 0x4e, 0x31]); // "GRN1"
const VERSION = 1;
const HEADER_LEN = 42; // magic(4) + version(1) + flags(1) + kdfIter(4) + salt(16) + iv(12) + ctLen(4)
const SALT_LEN = 16;
const IV_LEN = 12;
const GCM_TAG_LEN = 16;
const KDF_ITERATIONS = 600000;

// A stego image's header is trusted only as far as this range: without it, a
// crafted file could name an arbitrarily large iteration count and turn
// "enter the wrong passphrase" into a multi-minute hang. Checked before
// deriveKey ever runs, not after.
const MIN_KDF_ITERATIONS = 100000;
const MAX_KDF_ITERATIONS = 2000000;

// 40 megapixels, checked before either flow reads pixels back out of a
// canvas. Uncapped, a large-enough image turns getImageData into hundreds of
// megabytes this page never needed to allocate.
const MAX_PIXELS = 40_000_000;

const els = {
    carrierInput: document.getElementById('grain--carrier'),
    carrierDrop: document.getElementById('grain--carrier-drop'),
    carrierPreview: document.getElementById('grain--carrier-preview'),
    sampleBtn: document.getElementById('grain--sample'),
    sampleImg: document.getElementById('grain--sample-img'),
    payloadInput: document.getElementById('grain--payload'),
    payloadDrop: document.getElementById('grain--payload-drop'),
    payloadName: document.getElementById('grain--payload-name'),
    pass: document.getElementById('grain--pass'),
    passConfirm: document.getElementById('grain--pass-confirm'),
    passShow: document.getElementById('grain--pass-show'),
    passHint: document.getElementById('grain--pass-hint'),
    passStatus: document.getElementById('grain--pass-status'),
    capacity: document.getElementById('grain--capacity'),
    encodeBtn: document.getElementById('grain--encode'),
    encodeStatus: document.getElementById('grain--encode-status'),
    encodeResult: document.getElementById('grain--encode-result'),
    download: document.getElementById('grain--download'),
    stegoPreview: document.getElementById('grain--stego-preview'),
    stegoInput: document.getElementById('grain--stego'),
    stegoDrop: document.getElementById('grain--stego-drop'),
    stegoName: document.getElementById('grain--stego-name'),
    decodePass: document.getElementById('grain--decode-pass'),
    decodePassShow: document.getElementById('grain--decode-pass-show'),
    decodeBtn: document.getElementById('grain--decode'),
    decodeStatus: document.getElementById('grain--decode-status'),
    decodeResult: document.getElementById('grain--decode-result'),
    extracted: document.getElementById('grain--extracted'),
};

let carrierBitmap = null;
let carrierPreviewUrl = null; // revoked whenever the carrier changes, unless it's the sample image
let payloadFile = null;
let stegoBitmap = null;
let encoding = false;
let decoding = false;
let downloadUrl = null;
let extractedUrl = null;

function setStatus(el, message) {
    el.textContent = message;
}

// ---- Bit-level container I/O -------------------------------------------

function embedBits(data, bytes) {
    const totalBits = bytes.length * 8;
    let bitIndex = 0;
    for (let p = 0; p < data.length && bitIndex < totalBits; p += 4) {
        for (let c = 0; c < 3 && bitIndex < totalBits; c++) {
            const bit = (bytes[bitIndex >> 3] >> (7 - (bitIndex & 7))) & 1;
            data[p + c] = (data[p + c] & 0xfe) | bit;
            bitIndex++;
        }
    }
}

function extractBits(data, byteCount) {
    const out = new Uint8Array(byteCount);
    const totalBits = byteCount * 8;
    let bitIndex = 0;
    for (let p = 0; p < data.length && bitIndex < totalBits; p += 4) {
        for (let c = 0; c < 3 && bitIndex < totalBits; c++) {
            const bit = data[p + c] & 1;
            out[bitIndex >> 3] |= bit << (7 - (bitIndex & 7));
            bitIndex++;
        }
    }
    return out;
}

function buildHeader(kdfIter, salt, iv, ctLen) {
    const header = new Uint8Array(HEADER_LEN);
    const view = new DataView(header.buffer);
    header.set(MAGIC, 0);
    header[4] = VERSION;
    header[5] = 0; // flags, reserved
    view.setUint32(6, kdfIter, false);
    header.set(salt, 10);
    header.set(iv, 26);
    view.setUint32(38, ctLen, false);
    return header;
}

function parseHeader(bytes) {
    for (let i = 0; i < MAGIC.length; i++) {
        if (bytes[i] !== MAGIC[i]) {
            throw new Error(
                "That doesn't look like a Grain stego image — its header is " +
                'missing the expected signature. Either it was not hidden by ' +
                'this page, or it was re-saved by something that touched its pixels.'
            );
        }
    }
    const version = bytes[4];
    if (version !== VERSION) {
        throw new Error(
            `This file was hidden with header version ${version}, which this ` +
            `page (version ${VERSION}) does not understand.`
        );
    }
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    return {
        kdfIter: view.getUint32(6, false),
        salt: bytes.slice(10, 26),
        iv: bytes.slice(26, 38),
        ctLen: view.getUint32(38, false),
    };
}

// ---- Crypto --------------------------------------------------------------

async function deriveKey(passphrase, salt, iterations) {
    const keyMaterial = await crypto.subtle.importKey(
        'raw',
        new TextEncoder().encode(passphrase),
        'PBKDF2',
        false,
        ['deriveKey']
    );
    return crypto.subtle.deriveKey(
        { name: 'PBKDF2', salt, iterations, hash: 'SHA-256' },
        keyMaterial,
        { name: 'AES-GCM', length: 256 },
        false,
        ['encrypt', 'decrypt']
    );
}

// ---- Carrier / stego image loading ---------------------------------------

async function loadBitmap(source) {
    const bitmap = await createImageBitmap(source, {
        colorSpaceConversion: 'none',
        premultiplyAlpha: 'none',
    });
    if (bitmap.width * bitmap.height > MAX_PIXELS) {
        const pixels = bitmap.width * bitmap.height;
        bitmap.close();
        throw new Error(
            `That image is ${bitmap.width}x${bitmap.height} (${pixels.toLocaleString()} ` +
            `pixels), over this page's ${MAX_PIXELS.toLocaleString()}-pixel cap. Pick a smaller one.`
        );
    }
    return bitmap;
}

function bitmapToImageData(bitmap) {
    const canvas = document.createElement('canvas');
    // Sized from the decoded bitmap, not from any CSS layout size — there is
    // none, since this canvas is never inserted into the page.
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const ctx = canvas.getContext('2d', { colorSpace: 'srgb', willReadFrequently: true });
    ctx.drawImage(bitmap, 0, 0);
    return { canvas, ctx, imageData: ctx.getImageData(0, 0, canvas.width, canvas.height) };
}

function capacityBytes(bitmap) {
    return Math.floor((bitmap.width * bitmap.height * 3) / 8);
}

function updateCapacity() {
    if (!carrierBitmap) {
        els.capacity.textContent = '';
        return;
    }
    const bytes = capacityBytes(carrierBitmap);
    const usable = Math.max(0, bytes - HEADER_LEN - 2);
    els.capacity.textContent =
        `${carrierBitmap.width}x${carrierBitmap.height}. Can carry up to ` +
        `${usable.toLocaleString()} encrypted bytes (before its filename).`;
}

function setCarrier(bitmap, previewSrc, isObjectUrl) {
    if (carrierBitmap) carrierBitmap.close();
    carrierBitmap = bitmap;
    if (carrierPreviewUrl) {
        URL.revokeObjectURL(carrierPreviewUrl);
        carrierPreviewUrl = null;
    }
    if (isObjectUrl) carrierPreviewUrl = previewSrc;
    els.carrierPreview.src = previewSrc;
    els.carrierPreview.hidden = false;
    updateCapacity();
}

async function onCarrierFile(file) {
    setStatus(els.encodeStatus, '');
    try {
        const bitmap = await loadBitmap(file);
        setCarrier(bitmap, URL.createObjectURL(file), true);
    } catch (err) {
        setStatus(els.encodeStatus, err.message);
    }
}

async function useSample() {
    setStatus(els.encodeStatus, '');
    try {
        if (!els.sampleImg.complete) {
            await els.sampleImg.decode();
        }
        const bitmap = await loadBitmap(els.sampleImg);
        setCarrier(bitmap, els.sampleImg.src, false);
    } catch (err) {
        setStatus(els.encodeStatus, err.message);
    }
}

async function onStegoFile(file) {
    setStatus(els.decodeStatus, '');
    els.stegoName.textContent = `Chosen: ${file.name}`;
    try {
        if (stegoBitmap) stegoBitmap.close();
        stegoBitmap = await loadBitmap(file);
    } catch (err) {
        stegoBitmap = null;
        setStatus(els.decodeStatus, err.message);
    }
}

function onPayloadFile(file) {
    payloadFile = file;
    els.payloadName.textContent = `Chosen: ${file.name} (${file.size.toLocaleString()} bytes)`;
}

// ---- Drag and drop ---------------------------------------------------------

function wireDropZone(dropEl, inputEl, onFile) {
    dropEl.addEventListener('dragover', (event) => {
        event.preventDefault();
        dropEl.classList.add('grain--drop-over');
    });
    dropEl.addEventListener('dragleave', () => {
        dropEl.classList.remove('grain--drop-over');
    });
    dropEl.addEventListener('drop', (event) => {
        event.preventDefault();
        dropEl.classList.remove('grain--drop-over');
        const file = event.dataTransfer.files[0];
        if (file) onFile(file);
    });
    inputEl.addEventListener('change', () => {
        const file = inputEl.files[0];
        if (file) onFile(file);
    });
}

// ---- Passphrase show/hide ---------------------------------------------------

function wirePassShow(checkbox, ...inputs) {
    checkbox.addEventListener('change', () => {
        const type = checkbox.checked ? 'text' : 'password';
        for (const input of inputs) input.type = type;
    });
}

// ---- Encode ---------------------------------------------------------------

async function handleEncode() {
    if (encoding || els.encodeBtn.getAttribute('aria-disabled') === 'true') return;

    setStatus(els.encodeStatus, '');
    if (!carrierBitmap) {
        setStatus(els.encodeStatus, 'Pick a carrier image first.');
        return;
    }
    if (!payloadFile) {
        setStatus(els.encodeStatus, 'Pick a file to hide first.');
        return;
    }
    const passphrase = els.pass.value;
    if (!passphrase) {
        setStatus(els.encodeStatus, 'Enter a passphrase.');
        return;
    }
    if (passphrase !== els.passConfirm.value) {
        setStatus(els.encodeStatus, 'The two passphrases do not match.');
        return;
    }

    encoding = true;
    els.encodeBtn.setAttribute('aria-disabled', 'true');
    setStatus(els.encodeStatus, 'Encrypting…');

    try {
        const payloadBytes = new Uint8Array(await payloadFile.arrayBuffer());
        const nameBytes = new TextEncoder().encode(payloadFile.name);
        if (nameBytes.length > 0xffff) {
            throw new Error('That filename is too long to encode.');
        }

        const plaintext = new Uint8Array(2 + nameBytes.length + payloadBytes.length);
        new DataView(plaintext.buffer).setUint16(0, nameBytes.length, false);
        plaintext.set(nameBytes, 2);
        plaintext.set(payloadBytes, 2 + nameBytes.length);

        const capacity = capacityBytes(carrierBitmap);
        const ctLen = plaintext.length + GCM_TAG_LEN;
        if (HEADER_LEN + ctLen > capacity) {
            throw new Error(
                `The encrypted payload is ${(HEADER_LEN + ctLen).toLocaleString()} bytes, ` +
                `but this carrier can only hold ${capacity.toLocaleString()}. Pick a larger ` +
                `image or a smaller file.`
            );
        }

        const salt = crypto.getRandomValues(new Uint8Array(SALT_LEN));
        const iv = crypto.getRandomValues(new Uint8Array(IV_LEN));
        const key = await deriveKey(passphrase, salt, KDF_ITERATIONS);

        // The header is authenticated but not itself encrypted: passing it as
        // AES-GCM's additionalData binds every field in it — including the
        // salt and iv a decoder will trust — to this exact ciphertext, so
        // tampering with any of them fails the auth tag instead of quietly
        // decrypting to garbage.
        const header = buildHeader(KDF_ITERATIONS, salt, iv, ctLen);
        const ciphertext = new Uint8Array(
            await crypto.subtle.encrypt({ name: 'AES-GCM', iv, additionalData: header }, key, plaintext)
        );

        const container = new Uint8Array(HEADER_LEN + ciphertext.length);
        container.set(header, 0);
        container.set(ciphertext, HEADER_LEN);

        const { canvas, ctx, imageData } = bitmapToImageData(carrierBitmap);
        const data = imageData.data;

        // Force full opacity before embedding. A carrier with partial
        // transparency can have its RGB channels rewritten by
        // premultiplication when some browsers decode it back, which would
        // corrupt the low bit this page is about to set — fully opaque
        // pixels have no such path.
        for (let p = 0; p < data.length; p += 4) {
            data[p + 3] = 255;
        }

        embedBits(data, container);
        ctx.putImageData(imageData, 0, 0);

        const blob = await new Promise((resolve, reject) => {
            canvas.toBlob(
                (result) => (result ? resolve(result) : reject(new Error('Could not encode the PNG.'))),
                'image/png'
            );
        });

        if (downloadUrl) URL.revokeObjectURL(downloadUrl);
        downloadUrl = URL.createObjectURL(blob);
        els.download.href = downloadUrl;
        els.stegoPreview.src = downloadUrl;
        els.encodeResult.hidden = false;
        setStatus(
            els.encodeStatus,
            `Done. ${container.length.toLocaleString()} encrypted bytes hidden in a ` +
            `${canvas.width}x${canvas.height} PNG.`
        );
        els.download.focus();
    } catch (err) {
        setStatus(els.encodeStatus, err.message);
    } finally {
        encoding = false;
        els.encodeBtn.setAttribute('aria-disabled', 'false');
    }
}

// ---- Decode -----------------------------------------------------------------

async function handleDecode() {
    if (decoding || els.decodeBtn.getAttribute('aria-disabled') === 'true') return;

    setStatus(els.decodeStatus, '');
    if (!stegoBitmap) {
        setStatus(els.decodeStatus, 'Pick a stego PNG first.');
        return;
    }
    const passphrase = els.decodePass.value;
    if (!passphrase) {
        setStatus(els.decodeStatus, 'Enter the passphrase.');
        return;
    }

    decoding = true;
    els.decodeBtn.setAttribute('aria-disabled', 'true');
    setStatus(els.decodeStatus, 'Reading…');

    try {
        const { imageData } = bitmapToImageData(stegoBitmap);
        const data = imageData.data;
        const capacity = capacityBytes(stegoBitmap);

        if (capacity < HEADER_LEN) {
            throw new Error('That image is too small to hold a Grain header.');
        }

        const headerBytes = extractBits(data, HEADER_LEN);
        const { kdfIter, salt, iv, ctLen } = parseHeader(headerBytes);

        if (kdfIter < MIN_KDF_ITERATIONS || kdfIter > MAX_KDF_ITERATIONS) {
            throw new Error(
                `This file's header claims ${kdfIter.toLocaleString()} key-derivation rounds, ` +
                `outside the range this page will run (${MIN_KDF_ITERATIONS.toLocaleString()}–` +
                `${MAX_KDF_ITERATIONS.toLocaleString()}). Refusing rather than deriving a key ` +
                `at an attacker-chosen cost.`
            );
        }
        if (ctLen > capacity - HEADER_LEN) {
            throw new Error(
                "This file's header claims more hidden data than the image has room for — " +
                'it is not a genuine Grain stego PNG.'
            );
        }

        const container = extractBits(data, HEADER_LEN + ctLen);
        const ciphertext = container.slice(HEADER_LEN);

        const key = await deriveKey(passphrase, salt, kdfIter);
        let plaintext;
        try {
            plaintext = new Uint8Array(
                await crypto.subtle.decrypt({ name: 'AES-GCM', iv, additionalData: headerBytes }, key, ciphertext)
            );
        } catch {
            throw new Error(
                'Decryption failed. Either the passphrase is wrong, or this image was altered ' +
                'after Grain hid the file in it.'
            );
        }

        const view = new DataView(plaintext.buffer, plaintext.byteOffset, plaintext.byteLength);
        const nameLen = view.getUint16(0, false);
        const filename = new TextDecoder().decode(plaintext.slice(2, 2 + nameLen));
        const fileBytes = plaintext.slice(2 + nameLen);

        if (extractedUrl) URL.revokeObjectURL(extractedUrl);
        extractedUrl = URL.createObjectURL(new Blob([fileBytes]));
        els.extracted.href = extractedUrl;
        els.extracted.download = filename || 'grain-recovered';
        els.decodeResult.hidden = false;
        setStatus(els.decodeStatus, `Recovered "${filename}", ${fileBytes.length.toLocaleString()} bytes.`);
        els.extracted.focus();
    } catch (err) {
        setStatus(els.decodeStatus, err.message);
    } finally {
        decoding = false;
        els.decodeBtn.setAttribute('aria-disabled', 'false');
    }
}

// ---- Wiring -----------------------------------------------------------------

wireDropZone(els.carrierDrop, els.carrierInput, onCarrierFile);
wireDropZone(els.payloadDrop, els.payloadInput, onPayloadFile);
wireDropZone(els.stegoDrop, els.stegoInput, onStegoFile);
wirePassShow(els.passShow, els.pass, els.passConfirm);
wirePassShow(els.decodePassShow, els.decodePass);
els.sampleBtn.addEventListener('click', useSample);
els.encodeBtn.addEventListener('click', handleEncode);
els.decodeBtn.addEventListener('click', handleDecode);
