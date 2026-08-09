// Chief: a flyable neon dataspace built from three live public feeds.
//
// Same "boundary and renderer" shape as js/mocap.js and js/iris.js — this file
// decides nothing about what a story, an event or a CVE means, it only turns
// each into a position, a colour and a size, and asks Three.js to draw it. The
// boundary itself is different in kind from the other three, and is argued
// from scratch in specs/F06_CHIEF.md: this is the first file on the site that
// talks to a host it does not control, on an ongoing basis, for the life of
// the page. Three of them, in fact — Hacker News, GitHub and the NVD — each
// polled independently so that one going quiet or rate-limiting never stops
// the other two.
//
// Nothing a visitor does is ever sent to any of the three hosts. Every
// request is a plain, unauthenticated GET against a public read endpoint;
// the only traffic is what those endpoints already publish to anyone who asks.

import * as THREE from 'three';

const HN_TOPSTORIES_URL = 'https://hacker-news.firebaseio.com/v0/topstories.json';
const HN_ITEM_URL = (id) => `https://hacker-news.firebaseio.com/v0/item/${id}.json`;
const GITHUB_EVENTS_URL = 'https://api.github.com/events';
const NVD_CVE_URL = 'https://services.nvd.nist.gov/rest/json/cves/2.0';

const HN_COUNT = 14;
const HN_INTERVAL_MS = 90_000;

// GitHub's own /events response carries `X-Poll-Interval: 60` — see
// specs/F06_CHIEF.md#github-events. This is that header's value, not a guess.
const GH_INTERVAL_MS = 60_000;
const GH_EVENT_LIFESPAN_MS = 45_000;

// NVD's unauthenticated rate limit is 5 requests per rolling 30s. This page
// issues exactly one request per cycle, so 120s leaves a wide margin rather
// than chasing the limit.
const CVE_INTERVAL_MS = 120_000;
const CVE_WINDOW_MS = 24 * 60 * 60 * 1000;
const CVE_COUNT = 14;

const RING_HN = 13;
const RING_GH = 21;
const RING_CVE = 29;

const els = {
  gate: document.getElementById('chief--gate'),
  load: document.getElementById('chief--load'),
  status: document.getElementById('chief--status'),
  stage: document.getElementById('chief--stage'),
  main: document.getElementById('chief'),
  stop: document.getElementById('chief--stop'),
  recenter: document.getElementById('chief--recenter'),
};

const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');

/* -------------------------------------------------------------------------- */
/* Scene                                                                      */
/* -------------------------------------------------------------------------- */

let renderer = null;
let scene = null;
let camera = null;
let core = null;
let hnGroup = null;
let ghGroup = null;
let cveGroup = null;
let clock = null;
let animHandle = null;

function haloTexture(hex) {
  // A soft radial sprite stands in for bloom post-processing, which nothing
  // vendored here provides — vendor/three/examples/jsm has no postprocessing
  // directory. Drawn once per colour and reused by every sprite that needs
  // it, so this never runs per frame or per object.
  const size = 64;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = size;
  const ctx = canvas.getContext('2d');
  const gradient = ctx.createRadialGradient(size / 2, size / 2, 0, size / 2, size / 2, size / 2);
  const color = new THREE.Color(hex);
  const rgb = `${Math.round(color.r * 255)}, ${Math.round(color.g * 255)}, ${Math.round(color.b * 255)}`;
  gradient.addColorStop(0, `rgba(${rgb}, 0.9)`);
  gradient.addColorStop(1, `rgba(${rgb}, 0)`);
  ctx.fillStyle = gradient;
  ctx.fillRect(0, 0, size, size);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  return texture;
}

const halos = {
  amber: haloTexture(0xffb000),
  green: haloTexture(0x39ff88),
  magenta: haloTexture(0xff2ec4),
};

function setupScene() {
  scene = new THREE.Scene();
  scene.fog = new THREE.FogExp2(0x05000f, 0.016);

  camera = new THREE.PerspectiveCamera(60, els.stage.clientWidth / els.stage.clientHeight, 0.1, 400);
  camera.position.set(0, 6, 40);

  scene.add(new THREE.AmbientLight(0x442266, 1.1));

  // Two stacked grids rather than one, coloured differently, read as a
  // horizon line even with no ground/sky geometry at all.
  const floor = new THREE.GridHelper(140, 28, 0xff2ec4, 0x2a0b3d);
  floor.position.y = -10;
  scene.add(floor);
  const ceiling = new THREE.GridHelper(140, 28, 0x39c6ff, 0x0b2a3d);
  ceiling.position.y = 26;
  scene.add(ceiling);

  core = new THREE.Mesh(
    new THREE.IcosahedronGeometry(2.4, 1),
    new THREE.MeshBasicMaterial({ color: 0x39c6ff, wireframe: true }),
  );
  scene.add(core);

  hnGroup = new THREE.Group();
  ghGroup = new THREE.Group();
  cveGroup = new THREE.Group();
  scene.add(hnGroup, ghGroup, cveGroup);

  renderer = new THREE.WebGLRenderer({ canvas: els.stage, antialias: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  renderer.setClearColor(0x05000f, 1);
  resizeRenderer();
  window.addEventListener('resize', resizeRenderer);

  clock = new THREE.Clock();
}

function resizeRenderer() {
  const { clientWidth: w, clientHeight: h } = els.stage;
  if (!w || !h || !renderer) return;
  renderer.setSize(w, h, false);
  camera.aspect = w / h;
  camera.updateProjectionMatrix();
}

function teardownScene() {
  window.removeEventListener('resize', resizeRenderer);
  [hnGroup, ghGroup, cveGroup].forEach((group) => {
    if (!group) return;
    group.children.slice().forEach(disposeObject);
  });
  if (renderer) renderer.dispose();
  renderer = scene = camera = core = hnGroup = ghGroup = cveGroup = clock = null;
}

function disposeObject(obj) {
  obj.parent?.remove(obj);
  obj.geometry?.dispose?.();
  obj.material?.dispose?.();
}

/* -------------------------------------------------------------------------- */
/* Flight controls: WASD + arrow keys, no pointer lock                       */
/* -------------------------------------------------------------------------- */
//
// Pointer Lock traps the cursor and behaves inconsistently across browsers
// and iframes, and nothing here needs it: arrow keys turn the camera, WASD
// moves it along the direction it is already facing, and every one of those
// is a real keydown a keyboard-only visitor already has. That is the same
// reasoning js/iris.js's calibration track gives for staying off a drag
// gesture nothing guarantees a visitor can perform.

const keys = new Set();
const FLY_SPEED = 16; // units/second
const TURN_SPEED = 1.6; // radians/second
const euler = new THREE.Euler(0, 0, 0, 'YXZ');

function onKeyDown(e) {
  keys.add(e.code);
}
function onKeyUp(e) {
  keys.delete(e.code);
}

function updateFlight(dt) {
  euler.setFromQuaternion(camera.quaternion);
  if (keys.has('ArrowLeft')) euler.y += TURN_SPEED * dt;
  if (keys.has('ArrowRight')) euler.y -= TURN_SPEED * dt;
  if (keys.has('ArrowUp')) euler.x = Math.min(euler.x + TURN_SPEED * dt, Math.PI / 2 - 0.05);
  if (keys.has('ArrowDown')) euler.x = Math.max(euler.x - TURN_SPEED * dt, -Math.PI / 2 + 0.05);
  camera.quaternion.setFromEuler(euler);

  const forward = new THREE.Vector3(0, 0, -1).applyQuaternion(camera.quaternion);
  const right = new THREE.Vector3(1, 0, 0).applyQuaternion(camera.quaternion);
  const move = new THREE.Vector3();
  if (keys.has('KeyW')) move.add(forward);
  if (keys.has('KeyS')) move.sub(forward);
  if (keys.has('KeyD')) move.add(right);
  if (keys.has('KeyA')) move.sub(right);
  if (keys.has('Space')) move.y += 1;
  if (keys.has('ShiftLeft') || keys.has('ShiftRight')) move.y -= 1;
  if (move.lengthSq() > 0) {
    move.normalize().multiplyScalar(FLY_SPEED * dt);
    camera.position.add(move);
  }
}

function recenter() {
  camera.position.set(0, 6, 40);
  camera.quaternion.identity();
}

/* -------------------------------------------------------------------------- */
/* Hacker News: front-page stories as orbiting nodes                         */
/* -------------------------------------------------------------------------- */

const hnItems = new Map(); // id -> { mesh, halo, score, descendants }
const hnItemCache = new Map();

async function getJSON(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  return res.json();
}

async function fetchHN() {
  const ids = (await getJSON(HN_TOPSTORIES_URL)).slice(0, HN_COUNT);
  const items = await Promise.all(
    ids.map(async (id) => {
      if (hnItemCache.has(id)) return hnItemCache.get(id);
      const item = await getJSON(HN_ITEM_URL(id));
      hnItemCache.set(id, item);
      return item;
    }),
  );
  updateHNRing(items);
  return items;
}

function updateHNRing(items) {
  const seen = new Set();
  items.forEach((item, i) => {
    if (!item || item.id == null) return;
    seen.add(item.id);
    const score = item.score ?? 1;
    const descendants = item.descendants ?? 0;
    let entry = hnItems.get(item.id);
    const angle = (i / items.length) * Math.PI * 2;
    const radius = RING_HN;
    const height = Math.min(10, Math.log2(descendants + 1) * 1.6);
    const size = 0.35 + Math.min(1.1, Math.log2(score + 1) * 0.16);

    if (!entry) {
      const mesh = new THREE.Mesh(
        new THREE.OctahedronGeometry(1, 0),
        new THREE.MeshBasicMaterial({ color: 0xffb000 }),
      );
      const halo = new THREE.Sprite(
        new THREE.SpriteMaterial({ map: halos.amber, transparent: true, depthWrite: false, blending: THREE.AdditiveBlending }),
      );
      halo.scale.setScalar(6);
      mesh.add(halo);
      hnGroup.add(mesh);
      entry = { mesh, halo };
      hnItems.set(item.id, entry);
    }
    entry.mesh.position.set(Math.cos(angle) * radius, height, Math.sin(angle) * radius);
    entry.mesh.scale.setScalar(size);
    entry.score = score;
    entry.descendants = descendants;
  });
  for (const [id, entry] of hnItems) {
    if (!seen.has(id)) {
      disposeObject(entry.halo);
      disposeObject(entry.mesh);
      hnItems.delete(id);
    }
  }
}

function hnSummary() {
  if (hnItems.size === 0) return null;
  const scores = [...hnItems.values()].map((e) => e.score ?? 0);
  return `Hacker News: ${hnItems.size} front-page stories, top score ${Math.max(...scores)}.`;
}

/* -------------------------------------------------------------------------- */
/* GitHub: public events as a decaying stream                                */
/* -------------------------------------------------------------------------- */

const ghEvents = []; // { mesh, bornAt, type }
let ghRateLimitedUntil = 0;
let ghLastBatchSize = 0;
let ghLastBatchTopType = null;

const GH_COLORS = {
  PushEvent: 0x39ff88,
  WatchEvent: 0x39c6ff,
  ForkEvent: 0x6a8dff,
  PullRequestEvent: 0xb266ff,
  IssuesEvent: 0xffb000,
  CreateEvent: 0x39c6ff,
  default: 0xaaaaaa,
};

async function fetchGitHub() {
  if (performance.now() < ghRateLimitedUntil) return;
  const res = await fetch(GITHUB_EVENTS_URL);
  if (res.status === 403 || res.status === 429) {
    const reset = Number(res.headers.get('x-ratelimit-reset'));
    ghRateLimitedUntil = performance.now() + (reset ? Math.max(30_000, reset * 1000 - Date.now()) : 300_000);
    throw new Error(`GitHub rate-limited; backing off`);
  }
  if (!res.ok) throw new Error(`GitHub events: HTTP ${res.status}`);
  const events = await res.json();
  spawnGitHubEvents(events);
  return events;
}

function spawnGitHubEvents(events) {
  const counts = {};
  events.forEach((e) => { counts[e.type] = (counts[e.type] ?? 0) + 1; });
  ghLastBatchSize = events.length;
  ghLastBatchTopType = Object.entries(counts).sort((a, b) => b[1] - a[1])[0]?.[0] ?? null;

  const now = performance.now();
  events.slice(0, 60).forEach((event) => {
    const color = GH_COLORS[event.type] ?? GH_COLORS.default;
    const mesh = new THREE.Mesh(
      new THREE.TetrahedronGeometry(0.5, 0),
      new THREE.MeshBasicMaterial({ color }),
    );
    const angle = Math.random() * Math.PI * 2;
    const height = (Math.random() - 0.5) * 14;
    mesh.userData.angle = angle;
    mesh.userData.radius = RING_GH;
    mesh.position.set(Math.cos(angle) * RING_GH, height, Math.sin(angle) * RING_GH);
    ghGroup.add(mesh);
    ghEvents.push({ mesh, bornAt: now, type: event.type });
  });
}

function ageOutGitHubEvents(now) {
  for (let i = ghEvents.length - 1; i >= 0; i--) {
    const e = ghEvents[i];
    const age = now - e.bornAt;
    if (age > GH_EVENT_LIFESPAN_MS) {
      disposeObject(e.mesh);
      ghEvents.splice(i, 1);
    } else {
      e.mesh.material.opacity = 1 - age / GH_EVENT_LIFESPAN_MS;
      e.mesh.material.transparent = true;
    }
  }
}

function ghSummary() {
  if (ghLastBatchSize === 0) return null;
  return `GitHub: ${ghLastBatchSize} public events in the last update, mostly ${ghLastBatchTopType}.`;
}

/* -------------------------------------------------------------------------- */
/* NVD: recently modified CVEs as severity spikes                            */
/* -------------------------------------------------------------------------- */

const cveItems = new Map();
let cveHighest = null;

function severityColor(score) {
  if (score >= 9) return 0xff2ec4; // critical: magenta
  if (score >= 7) return 0xff5050; // high: red
  if (score >= 4) return 0xffb000; // medium: amber
  return 0x39ff88; // low: green
}

function isoNoMillis(date) {
  return date.toISOString().replace('Z', '');
}

async function fetchCVEs() {
  const end = new Date();
  const start = new Date(end.getTime() - CVE_WINDOW_MS);
  const url = `${NVD_CVE_URL}?lastModStartDate=${isoNoMillis(start)}&lastModEndDate=${isoNoMillis(end)}&resultsPerPage=${CVE_COUNT}`;
  const data = await getJSON(url);
  updateCVERing(data.vulnerabilities ?? []);
  return data;
}

function baseScore(cve) {
  const metrics = cve.metrics ?? {};
  for (const key of ['cvssMetricV31', 'cvssMetricV30', 'cvssMetricV2']) {
    if (metrics[key]?.[0]?.cvssData?.baseScore != null) return metrics[key][0].cvssData.baseScore;
  }
  return 0;
}

function updateCVERing(vulnerabilities) {
  const seen = new Set();
  let highest = null;
  vulnerabilities.forEach((v, i) => {
    const cve = v.cve;
    if (!cve) return;
    seen.add(cve.id);
    const score = baseScore(cve);
    if (!highest || score > highest.score) highest = { id: cve.id, score };
    const angle = (i / vulnerabilities.length) * Math.PI * 2;
    const height = 2 + score * 1.4;

    let entry = cveItems.get(cve.id);
    if (!entry) {
      const mesh = new THREE.Mesh(
        new THREE.ConeGeometry(0.5, 1, 6),
        new THREE.MeshBasicMaterial({ color: severityColor(score) }),
      );
      const halo = new THREE.Sprite(
        new THREE.SpriteMaterial({ map: halos.magenta, transparent: true, depthWrite: false, blending: THREE.AdditiveBlending }),
      );
      halo.scale.setScalar(5);
      halo.position.y = 0.5;
      mesh.add(halo);
      cveGroup.add(mesh);
      entry = { mesh };
      cveItems.set(cve.id, entry);
    }
    entry.mesh.position.set(Math.cos(angle) * RING_CVE, height / 2, Math.sin(angle) * RING_CVE);
    entry.mesh.scale.set(1, height, 1);
    entry.mesh.material.color.set(severityColor(score));
  });
  for (const [id, entry] of cveItems) {
    if (!seen.has(id)) {
      disposeObject(entry.mesh);
      cveItems.delete(id);
    }
  }
  cveHighest = highest;
}

function cveSummary() {
  if (!cveHighest) return null;
  return `NVD: ${cveItems.size} CVEs modified in the last day; highest severity ${cveHighest.id} at ${cveHighest.score.toFixed(1)}.`;
}

/* -------------------------------------------------------------------------- */
/* Polling: three independent loops, one failure never blocks another        */
/* -------------------------------------------------------------------------- */

const timers = [];

function scheduleLoop(fn, intervalMs, label) {
  let backoff = intervalMs;
  async function tick() {
    try {
      await fn();
      backoff = intervalMs;
    } catch (err) {
      console.warn(`Chief: ${label} fetch failed, backing off`, err);
      backoff = Math.min(backoff * 2, intervalMs * 8);
    }
    updateStatus();
    timers.push(setTimeout(tick, backoff));
  }
  tick();
}

function updateStatus() {
  const parts = [hnSummary(), ghSummary(), cveSummary()].filter(Boolean);
  els.status.textContent = parts.length
    ? parts.join(' ')
    : 'Waiting for the first response from Hacker News, GitHub and the NVD…';
}

function clearTimers() {
  timers.splice(0).forEach(clearTimeout);
}

/* -------------------------------------------------------------------------- */
/* Animation loop                                                            */
/* -------------------------------------------------------------------------- */

function animate() {
  animHandle = requestAnimationFrame(animate);
  const dt = clock.getDelta();
  const now = performance.now();

  updateFlight(dt);
  ageOutGitHubEvents(now);

  if (!reducedMotion.matches) {
    core.rotation.y += dt * 0.3;
    core.rotation.x += dt * 0.11;
    core.scale.setScalar(1 + Math.sin(now / 500) * 0.06);
    hnGroup.rotation.y += dt * 0.02;
    cveGroup.rotation.y -= dt * 0.015;
  }

  renderer.render(scene, camera);
}

/* -------------------------------------------------------------------------- */
/* Gate                                                                       */
/* -------------------------------------------------------------------------- */

function start() {
  els.load.setAttribute('aria-disabled', 'true');
  els.gate.hidden = true;
  els.main.hidden = false;
  setupScene();
  window.addEventListener('keydown', onKeyDown);
  window.addEventListener('keyup', onKeyUp);
  els.recenter.addEventListener('click', recenter);
  scheduleLoop(fetchHN, HN_INTERVAL_MS, 'Hacker News');
  scheduleLoop(fetchGitHub, GH_INTERVAL_MS, 'GitHub');
  scheduleLoop(fetchCVEs, CVE_INTERVAL_MS, 'NVD');
  animate();
}

function stop() {
  cancelAnimationFrame(animHandle);
  clearTimers();
  window.removeEventListener('keydown', onKeyDown);
  window.removeEventListener('keyup', onKeyUp);
  els.recenter.removeEventListener('click', recenter);
  keys.clear();
  hnItems.clear();
  hnItemCache.clear();
  ghEvents.length = 0;
  cveItems.clear();
  teardownScene();
  els.main.hidden = true;
  els.gate.hidden = false;
  els.load.removeAttribute('aria-disabled');
  els.status.textContent = '';
}

els.load.addEventListener('click', () => {
  if (els.load.getAttribute('aria-disabled') === 'true') return;
  start();
});
els.stop.addEventListener('click', stop);

// Hidden by default so a no-JS visitor never sees a button that can't do
// anything; this file running at all is the signal it's safe to reveal.
els.gate.hidden = false;
