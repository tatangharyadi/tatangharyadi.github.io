# F04: Twin, a downloadable 3D copy of the visitor's own face

**Status:** shipped. `twin.html`, `css/twin.css`, `js/twin.js`, the two
`assets/models/*` files, the vendored runtime
(`vendor/mediapipe/tasks-vision/`,
`vendor/three/examples/jsm/{loaders/OBJLoader.js,exporters/GLTFExporter.js,utils/TextureUtils.js}`),
`AGENTS.md`'s exception paragraph for `js/twin.js`, and the telemetry
mitigation below (`twin.html`'s CSP `connect-src` meta tag) are all merged to
`main` (PR #24, 2026-08-06). This document remains the design record those
files implement against, in the same role `F03_MOCAP.md` plays for
`mocap.html`.

F04-AC01 below — that the vendored `@mediapipe/tasks-vision` WASM bundle
loads and `FaceLandmarker.detectForVideo()` runs on a page served the way
GitHub Pages serves this site, with no COOP/COEP and no `SharedArrayBuffer`
— was the first thing confirmed before `js/twin.js` was written, and holds
in the shipped code.

---

## Overview

Twin turns a webcam capture of the visitor's own face into a personalized 3D
mesh with the visitor's own texture baked onto it, stylized with a small set
of non-generative filters, and downloaded as a `.glb` — all in the browser,
nothing uploaded, nothing kept once the tab closes unless the visitor
explicitly presses download.

This is a **digital twin**, not a puppeted avatar: unlike Echo, there is no
separate character and no rig driven by expression. The mesh Twin exports
*is* a copy of the visitor's own face, built from real geometry (their
landmark positions) and a real texture (their own captured video frame), not
a stand-in character reacting to blendshape values.

### Why this earns a new runtime instead of reusing Echo's

`js/mocap.js` calls LiteRT.js's low-level API directly — `loadAndCompile()`
and `model.run(tensor)` on a raw `.tflite` file, with no MediaPipe Tasks
wrapper in the loop. That gets BlazePose's 33 body landmarks and nothing
else: no blendshapes, no head-pose matrix, because those are computed by
MediaPipe's Tasks-layer `face_geometry` calculator, which LiteRT.js's
low-level surface does not run.

Twin's whole design — placing a side or profile capture's texture correctly
against the front capture, without hand-rolling a Procrustes/Kabsch pose fit
— depends on exactly that calculator's output:
`outputFacialTransformationMatrixes`. Rather than vendor a second thing under
LiteRT to get there, Twin vendors `@mediapipe/tasks-vision` directly and gets
the matrix as a plain opt-in flag on `FaceLandmarker.createFromOptions()`.
This is a second, independent ML runtime in the repo, argued on its own
terms rather than inheriting `js/mocap.js`'s LiteRT precedent — see
`AGENTS.md`'s framing that a new JavaScript exception is argued from
scratch, not from the last one.

### Where a visitor reaches it

Not a third masthead word. The masthead role is "Chief Technology Officer",
and both content words already carry a link — inventing a third would mean
rewriting copy that presently reads as ordinary prose to anyone who isn't
looking for a door. Twin is reached instead by a plain link from `mocap.html`
itself, once a visitor has already found Echo: the two features share a
camera-permission gate and an on-device-inference premise, so surfacing one
from the other costs nothing structural and needs no new "hidden word."
`twin.html` still gets its own entry in `sitemap.xml` and its own page, same
as `mocap.html` — it is just not a second easter egg.

A separate chooser/landing page for "pick Echo or Twin" ahead of both was
considered and rejected: it would add a third page, a third `sitemap.xml`
entry and a third nav-check exemption for a single decision point, and it
would force the masthead's `aria-label` on the "Officer" link — currently
specific ("Echo: puppet a 3D character with your own movement, on device")
— into something generic covering two features of very different
readiness. Echo is shipped; Twin, as of this spec, is not. Presenting them
as two equally-weighted doors overstates how done Twin is. The link lives
inside `mocap.html`, framed as "also try," not as a mode switch — something
close to *"Want a copy of your own face instead?"* — and is a plain `<a
href="twin.html">`, not an htmx trigger, for the same JavaScript-off reason
the masthead's two doors are plain links.

---

## Key files (none committed yet)

| File | Role |
| --- | --- |
| `twin.html` | The page: camera gate, capture controls, Three.js stage, stylize controls, download button, status region |
| `css/twin.css` | Stage/gate/capture layout. No colour of its own beyond the shared palette. |
| `js/twin.js` | Camera acquisition, Tasks Vision load, capture loop, mesh deform, texture bake, stylize, GLTFExporter, download |
| `vendor/mediapipe/tasks-vision/*` | Vendored `@mediapipe/tasks-vision`: JS glue + WASM binary |
| `assets/models/face-landmarker/face_landmarker.task` | MediaPipe's Face Landmarker bundle (detector + landmark + geometry submodels in one file) |
| `assets/models/canonical-face/canonical_face_model.obj` | MediaPipe's fixed-topology 468-vertex face mesh with baked-in UVs |
| `vendor/three/examples/jsm/loaders/OBJLoader.js` | Vendored OBJ loader for the canonical mesh — Echo only needed `GLTFLoader`, this needs `OBJLoader` too |
| `vendor/three/examples/jsm/exporters/GLTFExporter.js` | Vendored GLTF exporter, for the download |

`vendor/three/*` itself is already committed for Echo and is reused as-is.

---

## Architecture

### Milestone 1: front-only capture, no pose fit

```
visitor presses #twin--load
        │
        ├─ getUserMedia({video: true})
        ├─ FilesetResolver.forVisionTasks('vendor/mediapipe/tasks-vision/wasm')
        │        └─ FaceLandmarker.createFromOptions({ modelAssetPath: FACE_MODEL_URL,
        │                                                outputFaceBlendshapes: false,
        │                                                outputFacialTransformationMatrixes: false })
        └─ OBJLoader().loadAsync(CANONICAL_MESH_URL) → THREE.Mesh, canonical UVs intact

visitor presses #twin--capture-front
        │
   detectForVideo() over CALIBRATION_FRAMES tracked frames, averaged per-landmark
   (same reasoning as js/mocap-retarget.js's CALIBRATION_FRAMES: one frame is as
   exposed to noise as the signal it's meant to fix)
        │
   468 stabilized landmark positions ──► recenter + uniform-scale-fit onto the
   canonical mesh's own bounding size (no rotation solve needed — capture is
   already front-on) ──► replace each of the mesh's 468 vertex positions
        │
   draw the calibration-window's video frame to an offscreen canvas
        │
   for each mesh triangle: rasterize source = that frame's screen-space
   landmark xy for the triangle's 3 vertices, destination = the mesh's
   existing canonical UV coordinates for the same 3 vertices — a per-triangle
   affine warp, not a straight copy of the canonical UV layout onto the frame
        │
   triangles with no front-visible source (occiput, scalp, rear) keep a flat
   neutral fill baked in at asset-authoring time, not computed per-visitor
        │
   #twin--stylize applies a canvas/geometry filter (desaturate, posterize,
   flat-shaded toggle) to the baked texture and/or mesh material — no
   generative model, no network call
        │
   #twin--download: THREE.GLTFExporter → ArrayBuffer → Blob →
   URL.createObjectURL → <a download> click → revokeObjectURL
```

`FilesetResolver.forVisionTasks()`'s internal path builder appends
`vision_{wasm|wasm_nosimd}_internal.{js,wasm}` directly onto the base path
with no `wasm/` segment of its own — confirmed by reading the builder in
`vendor/mediapipe/tasks-vision/vision_bundle.mjs`. The base path passed in
above is therefore `.../tasks-vision/wasm`, matching where those four files
are actually vendored, not the parent `tasks-vision/` directory the model
and license file live in.

### Milestone 2: multi-angle capture, deferred until milestone 1 ships

Adds four more captures — two quarter-turns, two full profiles — each read
with `outputFacialTransformationMatrixes: true`. That matrix is head pose
relative to a stable reference frame, computed by MediaPipe's own
`face_geometry` calculator inside the Tasks bundle: no hand-rolled
Procrustes/Kabsch fit is needed here, because that is exactly the situation
Twin vendored Tasks Vision instead of reusing LiteRT to avoid.

Each additional capture's matrix places its partial texture bake into the
same canonical UV atlas the front capture already filled, extending real
(not generic) coverage toward the sides and — for the two profile captures —
close to ear-to-ear. **How far "close to" actually reaches is an unmeasured
number, not a design constant**: Face Landmarker needs visible facial
features to compute a landmark set at all, and stops returning a detection
past some yaw the model itself decides, empirically somewhere in the
neighbourhood of 80–90°, not written down anywhere as a guarantee. The
acceptance test for milestone 2 is to measure that angle on a real camera
with an overlay (the same discipline that resolved Echo's yaw bugs), and let
the genuinely-achieved coverage — not five assumed capture angles — decide
how much of the head stays real versus generic shroud.

Multi-angle capture in this design extends *texture* coverage only. It does
not refine the *geometry* — the deform step still uses only the front
capture's stabilized landmarks. Using parallax across captures to also
refine vertex positions is a real idea and a real amount of additional work;
it is deferred, not assumed into milestone 2's scope.

---

## Scope cuts

- **One mesh topology.** `canonical_face_model.obj` only. No per-visitor
  face-shape category, no jaw/skull proportion adjustment beyond moving the
  468 vertices FaceLandmarker actually reports.
- **No hair, no eyewear, no accessory geometry.** The canonical mesh has
  none; a visitor's own hair is not modeled and the scalp/rear stays part of
  the generic shroud regardless of what milestone 2 achieves at the sides.
- **No generative fill of any kind**, on the shroud or anywhere else. The
  neutral fallback fill is authored once, at asset time, and applied
  identically to every visitor — it does not attempt to guess what an
  individual visitor's hair or the back of their head looks like.
- **Milestone 2's geometry stays frontal-only**, deliberately, even once
  multi-angle texture lands — see above.
- **No recording, no server-side anything.** Same structural privacy claim
  as Echo: the capture frame, the landmarks and the exported mesh never
  cross a network boundary. The download is a local `Blob`, not a fetch.
- **No mobile-specific layout pass** beyond the standard breakpoints, same
  reasoning as Echo: revisit only if usage shows it matters.

---

## The DOM contract

Same shape as `#echo--*`: every id is looked up once, at module scope, in
`js/twin.js`.

| Id | Element | Contract |
| --- | --- | --- |
| `twin` | `section` or page root | |
| `twin--gate` | `div` | States what camera access is used for and that nothing leaves the tab, before the press |
| `twin--load` | `button type="button"` | Requests the camera and starts loading the runtime, model and canonical mesh |
| `twin--video` | `video` | `muted`, `playsinline` |
| `twin--capture-front` | `button type="button"` | Captures and averages the front-facing calibration window |
| `twin--stage` | `canvas` | Three.js render target showing the deformed, textured mesh |
| `twin--stylize` | `select` or radio group | Non-generative filter presets |
| `twin--download` | `button type="button"` | Triggers the GLTFExporter → Blob → `<a download>` sequence |
| `twin--status` | `p` | `role="status"`, `aria-live="polite"` |
| `twin--stop` | `button type="button"` | Releases the camera stream's tracks and stops the render loop |

None of these use the `disabled` property, for the same reason
`#echo--load` and `#ask--load` don't: each is on the page across a
multi-second permission prompt or model download, and disabling on press
strands a keyboard user on `<body>` for that entire window. `aria-disabled`
plus a re-entry guard flag, same as the other two features.

---

## The vendored bundle phones home, and the mitigation

`vendor/mediapipe/tasks-vision/vision_bundle.mjs` contains its own usage-
telemetry client, independent of anything Twin's code calls. Traced by
reading the minified source directly:

- A per-task usage tracker (the class holding call latencies, internally
  `Dh`) is instantiated unconditionally inside the same async path that
  creates any vision task — the same `createFromOptions()` call Twin makes
  for `FaceLandmarker`. There is no option on `BaseOptions` or anywhere else
  in the consumer-facing API to skip this; grepping the bundle for
  `disableTelemetry`, `optOut`, or any similarly-named flag returns nothing.
- That tracker's constructor immediately creates a log batcher
  (internally `Fh`), which starts a 60-second `setInterval` the moment it
  exists and, on each tick, `fetch()`s a POST to
  `https://odml.pa.googleapis.com/v1/log` with an `x-goog-api-key` header —
  whatever value was passed in, which for Twin's planned code (no API key is
  ever configured, per this spec) is empty or absent.
- The batcher only sends when its queue is non-empty. The queue is filled by
  a second, per-inference path: roughly every 30 seconds of wall-clock time
  since the last flush, the next `detectForVideo()` call pushes an
  aggregated latency entry. Twin calls `detectForVideo()` every rendered
  frame during calibration capture, so the 30-second threshold is
  reached on ordinary use, not just under sustained/batch inference.

The practical consequence: creating a `FaceLandmarker` and calling
`detectForVideo()` for more than ~30 seconds — which milestone 1's
calibration-window averaging does by design — results in an outbound
`fetch()` to a Google-controlled host, with no code-level way to prevent it
from inside `js/twin.js`. This is a materially different situation from
Echo's LiteRT.js or Ask's `onnxruntime`: those runtimes only ever fetch their
own vendored, same-origin assets once; this runtime, once vendored, attempts
a genuine cross-origin network call during normal operation regardless of
what Twin's own code does or doesn't send it.

**Mitigation: a page-level `Content-Security-Policy` meta tag on
`twin.html`.** `<meta http-equiv="Content-Security-Policy" content="connect-src
'self' blob:">` makes the browser itself refuse the `fetch()` before it
leaves the tab — `'self'` covers the same-origin model/WASM fetches
`FilesetResolver` and `FaceLandmarker.createFromOptions()` need, `blob:`
covers anything in the vendored loader or `GLTFExporter`'s download path
that resolves a `blob:` URL, and the omission of `odml.pa.googleapis.com`
from either list is what turns F04-AC03 from a claim about what Twin's own
code does into a claim enforced by the browser regardless of what the
vendored dependency tries to do. This is why Twin needs a CSP meta tag
neither `mocap.html` nor `ask.html` needed: their vendored runtimes never
attempt an outbound fetch in the first place, so there was nothing for a CSP
to block.

**This shows up as a console line, on purpose.** A blocked request logs a
CSP violation to the console. AGENTS.md's verification step 5 calls for a
clean console; this is the one deliberate exception, and the reason is the
inverse of what a clean-console rule is usually protecting against — the
violation line is the visible proof the block fired, not evidence something
is broken. Verifying F04-AC03 by hand means confirming that line is present
(not merely confirming the network panel shows no successful request, since
a same-origin-only network panel and a CSP-blocked network panel look
identical there) and confirming no other, unexpected console output
accompanies it.

---

## Acceptance criteria

| ID | Criterion | Evidence |
| --- | --- | --- |
| F04-AC01 | The vendored `@mediapipe/tasks-vision` WASM bundle loads and `detectForVideo()` runs with no `SharedArrayBuffer` and no COOP/COEP response headers. | Human, real browser, served the way GitHub Pages serves it — **run this before anything else in this spec is built** |
| F04-AC02 | `FaceLandmarker`'s reported landmark count and index order match `canonical_face_model.obj`'s vertex count and order one-for-one before any deform code assumes it. | Human: log `landmarks.length` and diff against the loaded mesh's vertex count — do not assume 468; MediaPipe's newer landmark sets include iris points some canonical assets don't, and Echo already has a precedent (F03-AC09) for a silent, unthrown mismatch here |
| F04-AC03 | The video frame, the landmarks and the exported mesh are never transmitted anywhere. | Enforced by `twin.html`'s CSP `connect-src` meta tag (see "The vendored bundle phones home, and the mitigation" above), not by the vendored bundle's own behaviour — verify both a CSP-violation console line for the blocked `odml.pa.googleapis.com` request and no successful third-party request in the network panel, including at download |
| F04-AC04 | The download is a `Blob` URL consumed by a same-page `<a download>`, not a `window.open` or a fetch. | Structural: `js/twin.js` |
| F04-AC05 | None of `#twin--load`, `#twin--capture-front`, `#twin--download`, `#twin--stop` use the `disabled` property; focus survives camera grant, load, capture and export. | Human, keyboard traversal |
| F04-AC06 | `twin.html` has no `nav-links--container`, matching `ask.html`/`mocap.html`'s precedent, so `check_repo.py`'s nav check does not need updating for it. | Structural, `scripts/check_repo.py` |
| F04-AC07 | `twin.html` is listed in `sitemap.xml`. | `scripts/check_repo.py`, CI |
| F04-AC08 | Contrast holds at 4.5:1 for text in both flavours, checked against Latte. | Human, per colour scheme |
| F04-AC09 (milestone 2 only) | The real yaw angle at which `FaceLandmarker` stops returning a detection is measured on a live camera, not assumed at 80–90°. | Human, overlay logging actual yaw at last-successful-detection |

---

## Deferred

| Item | Note |
| --- | --- |
| Multi-angle capture (milestone 2 as a whole) | Depends on F04-AC09's measurement; milestone 1 ships and is a complete, downloadable artifact without it. |
| Parallax-based geometry refinement from multiple captures | Real additional work, not folded into milestone 2's texture-only scope. |
| Hair, eyewear, accessory modeling | Canonical mesh has none; would need a different asset entirely. |
| A stylized-avatar-driven-by-expression mode (reusing Echo's retargeting pattern against Face Landmarker's blendshapes) | A different, less advanced feature considered and set aside earlier in this feature's design discussion — Twin is a copy of the visitor's own face, not a puppeted character. |
