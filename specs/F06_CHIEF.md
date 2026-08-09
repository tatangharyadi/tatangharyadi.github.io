# F06: Chief, a flyable dataspace built from live feeds

## Overview

Chief is a full-page, flyable 3D scene, reached from the "Chief" word in the
masthead's job title the same way Iris is reached from "Technology" and the
sailing sim from "Officer". It exists to demonstrate one specific claim —
"keeping up to date with technology" — visually and continuously rather than
as a sentence on the portfolio page, by rendering three independent public
feeds as objects a visitor can fly among:

- **Hacker News** front-page stories, as orbiting nodes sized by score and
  raised by comment count.
- **GitHub's public events feed**, as short-lived particles that spawn,
  drift and fade.
- **The NVD's CVE feed**, as severity spikes coloured by CVSS base score.

None of the three is simulated, cached-at-build-time, or replayed from a
snapshot. Each is fetched live, on its own schedule, for as long as the page
stays open, which is the point: a static screenshot of "what technology
looked like on the day this shipped" would not demonstrate staying current
with it.

## Key files

- `chief.html` — page shell: meta/CSP, gate, canvas stage, controls.
- `js/chief.js` — the sixth JS-file exception (see
  [AGENTS.md](../AGENTS.md)): fetches all three feeds, builds and flies the
  Three.js scene, degrades each source independently.
- `css/chief.css` — layout/chrome only, built entirely from `css/style.css`'s
  shared custom properties. The scene's neon palette is not here; see
  [Why the palette lives in js/chief.js, not css/chief.css](#why-the-palette-lives-in-jschiefjs-not-csschiefcss).
- `vendor/three/three.module.min.js` and
  `vendor/three/examples/jsm/` — reused unchanged from `js/mocap.js`.
  Nothing new is vendored for this page.

## Architecture

`js/chief.js` is a boundary and a renderer, the same shape `js/mocap.js` and
`js/iris.js` already use, applied to three plain HTTP endpoints instead of a
camera:

```
Hacker News Firebase API ---\
GitHub public events API ----+--> js/chief.js --> Three.js scene --> <canvas>
NVD CVE API 2.0 -------------/
```

Each source is its own independent polling loop (`scheduleLoop()`), each with
its own interval, its own exponential backoff on failure, and its own patch
of the scene (`hnGroup`, `ghGroup`, `cveGroup`). One source going quiet or
rate-limiting never blocks or clears another — a design decision made
explicit in [Graceful degradation, per source](#graceful-degradation-per-source)
because it is the entire mitigation for the risk this page takes on.

### Hacker News: front-page stories as orbiting nodes

Polled every 90 seconds. `topstories.json` returns up to 500 story IDs; the
first 14 are fetched individually via `item/{id}.json` and cached by ID for
the life of the page, since a story's own text never changes, only its score
and descendant count. Each story becomes an octahedron on the inner ring:
angle by front-page position, height by `log2(descendants + 1)`, size by
`log2(score + 1)`. Logarithms, not linear scales, because a 4,000-point story
and a 400-point story are both "doing very well" and should read as
neighbours, not as a spike ten times taller than the next node over.

### GitHub: public events as a decaying stream

Polled every 60 seconds — not a guess: GitHub's own `/events` response
carries an `X-Poll-Interval: 60` header, and this constant is that value.
Each response (up to 30 events, unauthenticated) spawns a tetrahedron on the
middle ring, coloured by event type (`PushEvent` green, `PullRequestEvent`
violet, `IssuesEvent` amber, and so on), and each one fades and is disposed
45 seconds after it spawns regardless of the next poll. The result reads as
weather, not as an accumulating pile: the scene shows what is happening on
GitHub right now, not everything that has ever happened since the page
loaded.

Unauthenticated requests to this endpoint are rate-limited to 60/hour, which
this page's 60-second interval sits exactly at. A `403`/`429` response is
read for an `X-RateLimit-Reset` header and the next attempt is deferred to
that reset time (or 5 minutes, if the header is absent), rather than
retrying into the same limit.

### NVD: recently modified CVEs as severity spikes

Polled every 120 seconds against `lastModStartDate`/`lastModEndDate` set to
the trailing 24 hours, requesting the 14 most recently modified records. Each
becomes a cone on the outer ring, height and colour both driven by CVSS base
score (preferring v3.1, falling back to v3.0 then v2 — see `baseScore()`),
so severity is legible at a glance from height alone even before colour
registers.

The NVD's unauthenticated limit is 5 requests per rolling 30 seconds. This
page issues exactly one request per 120-second cycle, which is nowhere near
that limit — the wide margin is deliberate, since NVD's API sits behind
Cloudflare and this page has no way to negotiate a higher limit if it were
ever throttled.

### Graceful degradation, per source

`scheduleLoop()` wraps each source's fetch in its own try/catch. A failure
logs a console warning, doubles that source's own backoff (capped at 8x its
base interval), and reschedules — it never touches the other two loops or
throws past its own tick. The aria-live status line
(`updateStatus()`) reports whichever sources have data at all; a visitor who
loads the page while GitHub is rate-limited still sees Hacker News and NVD
content and a status line that says so, not a page that has failed to start.

### The flight camera has no vendored dependency

`vendor/three/examples/jsm/controls/` carries only `OrbitControls.js`, which
orbits a fixed point rather than flying — wrong shape for "a person can fly
through" a scene with three concentric rings extending in every direction.
Three.js itself ships `FlyControls` and `PointerLockControls` as examples,
neither of which is vendored here, and vendoring either for one page would be
a second, unrelated exception stacked on top of this one.

Instead the controller is ~30 lines against primitives `three.module.min.js`
already exports: `THREE.Euler` (order `'YXZ'`, so yaw and pitch never gimbal
into each other) accumulates look direction from the arrow keys, and WASD
moves the camera along the forward/right vectors that orientation implies.
Space and Shift move along world-up. This is also the more accessible
choice, not just the cheaper one: Pointer Lock traps the cursor and behaves
inconsistently inside iframes and across browsers, and every interaction
this scheme needs is a `keydown` a keyboard-only visitor already has — no
pointer, no drag gesture, no captured cursor. A "Recenter" button resets
position and orientation to the start pose, for a visitor who has flown far
enough to lose their bearings.

### Bloom is faked with a canvas sprite, not a postprocessing pass

`vendor/three/examples/jsm/` has no `postprocessing/` directory, so
`UnrealBloomPass` is not available without vendoring a new module.
`haloTexture()` draws a radial gradient onto an offscreen `<canvas>` once per
colour, wraps it as a `THREE.CanvasTexture`, and every glowing node adds one
additively-blended `THREE.Sprite` using that shared texture. Three textures
total, generated once at module load, reused by every node of that colour —
not per-frame, not per-object.

### Why the palette lives in js/chief.js, not css/chief.css

`css/game.css` earns a "Mocha-only, literal hex" exception because it is
still a stylesheet that `scripts/check_palette.py` would otherwise need to
special-case; that script hardcodes exactly five places it checks
(`css/style.css`, `404.html`, `DESIGN.md`'s front matter, the `theme-color`
metas, and `css/game.css` itself via `check_game_css()`). Rather than argue a
second, similarly-named exception into that script, `css/chief.css` stays
entirely on the shared custom properties (`var(--bg-chrome)`, `var(--text)`,
`var(--accent-text)`, and so on) for every pixel of page chrome — heading,
gate, buttons, status text — exactly like `css/iris.css` already does. The
neon cyberpunk colours a visitor actually sees in the scene are literal hex
values passed straight to `THREE.Color`/materials/lights/fog inside
`js/chief.js`, which sits outside the palette-tracking system entirely, the
same way `js/iris.js`'s and `js/mocap.js`'s own in-canvas colours already do.
`scripts/check_palette.py` needs no changes for this page to exist.

## The site-first cross-origin connect-src

This is the first page on the site whose entire premise requires talking,
repeatedly, for as long as it is open, to hosts this site does not control.
That is a real rule-break, not a technicality — [AGENTS.md](../AGENTS.md)
already refuses a webfont CDN on exactly this ground, and Iris's own
exception (a single vendored ML runtime that turns out to phone home despite
running same-origin) is a leak to be closed, not a feature to be built
around. Chief asks for the opposite: the network calls are not a bug to
contain, they are the content.

The argument for allowing it rests on three properties, each concrete rather
than aspirational:

1. **Every request is a plain, unauthenticated GET against a public,
   read-only endpoint that already answers anyone who asks.** Nothing a
   visitor does — no keystroke, no camera frame, no query — is ever sent to
   any of the three hosts. This is the same structural property `js/ask.js`
   and `js/mocap.js` already lean on for privacy, applied here to prove the
   opposite direction: not "nothing leaves", but "nothing that leaves came
   from the visitor".
2. **The page states this plainly before it starts, and treats it as
   equivalent in kind to a hardware permission prompt.** `chief.html`'s gate
   describes the three ongoing requests in the same register `iris.html`'s
   gate describes camera access, because that is the more honest framing:
   what is being asked for here is network access instead of hardware
   access, not a lesser thing that needs a lighter warning.
3. **A page-level CSP makes the boundary real, not just documented.**
   `chief.html`'s `Content-Security-Policy` meta tag sets
   `connect-src 'self' https://hacker-news.firebaseio.com
   https://api.github.com https://services.nvd.nist.gov` — `'self'` for the
   vendored Three.js import and this page's own assets, and exactly the
   three feed hosts named above and nowhere else. Any other cross-origin
   request this page might ever accidentally introduce is refused by the
   browser outright, the same enforcement mechanism `iris.html` uses to
   contain its own leak.

The risk this still leaves open — three external services this site does
not operate, each free to go down, rate-limit or change shape without
notice — is accepted, not engineered away, because engineering it away (a
server-side proxy, a cached snapshot) would remove the exact property the
page exists to show. What is engineered is the fallout: see
[Graceful degradation, per source](#graceful-degradation-per-source). A
visitor who loads this page during an outage of any one service sees the
other two and a status line that says so, never a broken page.

## The DOM contract

`js/chief.js` looks up a fixed set of IDs and does nothing if any is absent:
`chief--gate`, `chief--load`, `chief--status`, `chief--stage`, `chief`,
`chief--stop`, `chief--recenter`. `chief.html` is the only page expected to
provide them. The gate/main split mirrors `iris.html`: the gate stays
`hidden` until JavaScript is confirmed to run, `#chief` stays `hidden` until
"Start the flight" is pressed, and `#chief--status` is an
`aria-live="polite"` region that carries every state change a sighted
visitor would otherwise only get from watching the scene — waiting for first
data, a summary per source once it arrives, and any source that has fallen
back to backoff.

No control on this page is ever given a `disabled` attribute; the load
button uses `aria-disabled` plus a guard flag while the scene is being torn
up or brought up, the same pattern `js/iris.js`'s and `js/ask.js`'s own
load/stop buttons already use.

## Acceptance criteria

- Loading `chief.html` with JavaScript disabled shows the `noscript` notice
  and nothing else broken.
- Pressing "Start the flight" requests all three feeds and begins rendering
  within one polling cycle of the slowest source.
- Blocking any one of the three hosts (e.g. via browser devtools request
  blocking) leaves the other two updating and produces a status line that
  still names all sources with data, not a stalled or blank page.
- The scene is fully navigable with a keyboard alone: arrow keys to look,
  WASD to move, Space/Shift for vertical, "Recenter" to reset, "Stop and
  disconnect" to end the session and release all three polling loops.
- With `prefers-reduced-motion: reduce` set, the core's idle rotation/pulse
  and the two data rings' ambient rotation stop; camera movement driven by
  an actual keypress still responds, because that motion is visitor-
  initiated, not ambient.
- Text contrast for all chrome (headings, gate copy, buttons, status text)
  meets 4.5:1 in both color schemes, inherited automatically from
  `css/style.css`'s shared custom properties.
- `chief.html` is listed in `sitemap.xml` and reachable from the masthead's
  "Chief" word.

## Deferred

- No persistence of any fetched data across page loads or between polling
  cycles beyond what each source's own loop keeps in memory; a reload starts
  from nothing, by design, since "current" is the entire point.
- No mobile-specific control scheme. Flight requires a physical keyboard;
  the gate's "Start the flight" still works on touch devices and the scene
  still renders and updates, but nothing repositions the camera without
  keyboard input. A touch-drag-to-look scheme is a real option for later but
  is not this page's first version.
- No attempt to reconcile or cross-reference the three feeds against each
  other (e.g. surfacing a CVE that is also trending on Hacker News). Each
  ring is deliberately independent; correlating them is a different,
  larger feature.
