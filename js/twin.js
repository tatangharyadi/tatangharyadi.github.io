// Twin: the browser half.
//
// Same shape as js/mocap.js on purpose — see specs/F04_TWIN.md. This file is
// a boundary and a renderer: it asks the browser for a camera, asks
// MediaPipe Tasks Vision's FaceLandmarker to turn a frame into face
// landmarks, deforms a canonical mesh to match them, bakes the calibration
// frame onto it as a texture, and asks Three.js to draw and export the
// result. It does not run a second ML runtime by choice — js/mocap.js's
// LiteRT.js has no equivalent of Tasks Vision's face_geometry output, which
// is why this file exists instead of extending that one; see
// specs/F04_TWIN.md#why-this-earns-a-new-runtime-instead-of-reusing-echos.
//
// Nothing captured here is sent anywhere the visitor did not already trust:
// the model, the runtime and the mesh are all fetched same-origin, once, and
// twin.html's own Content-Security-Policy meta tag refuses any other
// connection outright — including the vendored bundle's own telemetry
// fetch(). See specs/F04_TWIN.md#the-vendored-bundle-phones-home-and-the-mitigation.

import * as THREE from 'three';
import { OBJLoader } from '../vendor/three/examples/jsm/loaders/OBJLoader.js';
import { GLTFExporter } from '../vendor/three/examples/jsm/exporters/GLTFExporter.js';
import { FaceLandmarker, FilesetResolver } from '../vendor/mediapipe/tasks-vision/vision_bundle.mjs';

const WASM_BASE = 'vendor/mediapipe/tasks-vision/wasm';
const FACE_MODEL_URL = 'assets/models/face-landmarker/face_landmarker.task';
const CANONICAL_MESH_URL = 'assets/models/canonical-face/canonical_face_model.obj';

// Same value, same justification as js/mocap-retarget.js's own
// CALIBRATION_FRAMES: one frame is as exposed to noise as the signal it is
// meant to fix, so the front-facing capture averages this many tracked
// frames rather than trusting whichever single one happened to land.
const CALIBRATION_FRAMES = 30;

const els = {
  gate: document.getElementById('twin--gate'),
  load: document.getElementById('twin--load'),
  status: document.getElementById('twin--status'),
  main: document.getElementById('twin'),
  video: document.getElementById('twin--video'),
  stage: document.getElementById('twin--stage'),
  captureFront: document.getElementById('twin--capture-front'),
  stylize: document.getElementById('twin--stylize'),
  download: document.getElementById('twin--download'),
  stop: document.getElementById('twin--stop'),
};

/* -------------------------------------------------------------------------- */
/* Camera + model loading                                                    */
/* -------------------------------------------------------------------------- */

async function startCamera() {
  const stream = await navigator.mediaDevices.getUserMedia({
    video: { facingMode: 'user', width: { ideal: 640 }, height: { ideal: 480 } },
    audio: false,
  });
  els.video.srcObject = stream;
  await els.video.play();
  return stream;
}

async function loadFaceLandmarker() {
  const filesetResolver = await FilesetResolver.forVisionTasks(WASM_BASE);
  return FaceLandmarker.createFromOptions(filesetResolver, {
    baseOptions: { modelAssetPath: FACE_MODEL_URL },
    runningMode: 'VIDEO',
    numFaces: 1,
    outputFaceBlendshapes: false,
    outputFacialTransformationMatrixes: false,
  });
}

/* -------------------------------------------------------------------------- */
/* Canonical mesh                                                             */
/* -------------------------------------------------------------------------- */

// OBJLoader's own object model dedupes nothing for this file: every face in
// canonical_face_model.obj already maps each vertex to exactly one UV (no
// seams — verified by walking the file: 468 v, 468 vt, every f line pairing
// a given v index with the same vt index everywhere it appears), so building
// straight off the parsed BufferGeometry's own position/uv attributes is
// exact, not an approximation of the raw file.
async function loadCanonicalMesh() {
  const loader = new OBJLoader();
  const object = await loader.loadAsync(CANONICAL_MESH_URL);
  const mesh = object.children.find((child) => child.isMesh);
  if (!mesh) throw new Error('canonical_face_model.obj parsed with no mesh in it');
  const geometry = mesh.geometry;
  const positionAttr = geometry.attributes.position;

  // FaceLandmarker's landmark count is read off the actual result at capture
  // time and diffed against this, never assumed to be 468 — see F04-AC02 and
  // the note in captureFront() below. positionAttr.count is this mesh's own
  // vertex count, read the same way rather than hardcoded.
  return { geometry, vertexCount: positionAttr.count };
}

/* -------------------------------------------------------------------------- */
/* Landmarks -> mesh deform                                                  */
/* -------------------------------------------------------------------------- */

function boundsOf(getX, getY, getZ, count) {
  let minX = Infinity, minY = Infinity, minZ = Infinity;
  let maxX = -Infinity, maxY = -Infinity, maxZ = -Infinity;
  for (let i = 0; i < count; i++) {
    const x = getX(i), y = getY(i), z = getZ(i);
    if (x < minX) minX = x;
    if (x > maxX) maxX = x;
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
    if (z < minZ) minZ = z;
    if (z > maxZ) maxZ = z;
  }
  return {
    center: { x: (minX + maxX) / 2, y: (minY + maxY) / 2, z: (minZ + maxZ) / 2 },
    size: Math.max(maxX - minX, maxY - minY, maxZ - minZ),
  };
}

// Recenters and uniformly scale-fits the averaged landmarks onto the
// canonical mesh's own bounding size, then writes the result straight into
// the mesh's existing position attribute — no rotation solve, because
// captureFront() only ever runs against a front-on frame (F04-AC02's whole
// reason for existing: this deform is only valid when that holds). Landmark
// x/y are image-space (y down, z away from camera in MediaPipe's convention);
// the mesh's own space has y up, so both are flipped here rather than in the
// landmarks themselves.
function deformToLandmarks(geometry, vertexCount, landmarks) {
  const canonicalPos = geometry.attributes.position;
  const canon = boundsOf(
    (i) => canonicalPos.getX(i),
    (i) => canonicalPos.getY(i),
    (i) => canonicalPos.getZ(i),
    vertexCount
  );
  const lm = boundsOf(
    (i) => landmarks[i].x,
    (i) => landmarks[i].y,
    (i) => landmarks[i].z,
    vertexCount
  );
  const scale = canon.size / lm.size;

  for (let i = 0; i < vertexCount; i++) {
    const p = landmarks[i];
    canonicalPos.setXYZ(
      i,
      canon.center.x + (p.x - lm.center.x) * scale,
      canon.center.y - (p.y - lm.center.y) * scale,
      canon.center.z - (p.z - lm.center.z) * scale
    );
  }
  canonicalPos.needsUpdate = true;
  geometry.computeVertexNormals();
}

/* -------------------------------------------------------------------------- */
/* Texture bake                                                              */
/* -------------------------------------------------------------------------- */

const BAKE_SIZE = 1024;

// Non-front-visible triangles (scalp, ears, underside of the jaw) have no
// pixels in a single front-on frame to draw, so they fall back to a flat
// fill rather than being left transparent or smeared from whatever UV space
// happens to be nearby. Computed once per bake from the *deformed* mesh's
// own face normals, not authored as a separate asset — F04's spec allows an
// asset-time fallback, but a per-visitor flat fill needs nothing shipped
// that a mismatched capture (different face shape, different framing) could
// go stale against.
const FALLBACK_FILL = '#c9a98c';
const FRONT_FACING_THRESHOLD = 0.12;

function bakeTexture(geometry, vertexCount, videoFrame, landmarks) {
  const canvas = document.createElement('canvas');
  canvas.width = BAKE_SIZE;
  canvas.height = BAKE_SIZE;
  const ctx = canvas.getContext('2d');
  ctx.fillStyle = FALLBACK_FILL;
  ctx.fillRect(0, 0, BAKE_SIZE, BAKE_SIZE);

  const uvAttr = geometry.attributes.uv;
  const posAttr = geometry.attributes.position;
  const index = geometry.index;
  const triCount = index ? index.count / 3 : vertexCount / 3;
  const vIndex = (t, k) => (index ? index.getX(t * 3 + k) : t * 3 + k);

  const frameW = videoFrame.width;
  const frameH = videoFrame.height;

  for (let t = 0; t < triCount; t++) {
    const i0 = vIndex(t, 0), i1 = vIndex(t, 1), i2 = vIndex(t, 2);

    // Front-facing test uses the *deformed* geometry's own normal, computed
    // fresh per triangle rather than read from an attribute that
    // computeVertexNormals() only ever wrote as a per-vertex average.
    const ax = posAttr.getX(i1) - posAttr.getX(i0);
    const ay = posAttr.getY(i1) - posAttr.getY(i0);
    const az = posAttr.getZ(i1) - posAttr.getZ(i0);
    const bx = posAttr.getX(i2) - posAttr.getX(i0);
    const by = posAttr.getY(i2) - posAttr.getY(i0);
    const bz = posAttr.getZ(i2) - posAttr.getZ(i0);
    const nx = ay * bz - az * by;
    const ny = az * bx - ax * bz;
    const nz = ax * by - ay * bx;
    const len = Math.hypot(nx, ny, nz) || 1;
    if (nz / len < FRONT_FACING_THRESHOLD) continue; // left as the flat fallback fill

    // Source triangle in the calibration frame's own pixel space, from the
    // matching landmarks (image-space, origin top-left, already normalized
    // 0..1 by FaceLandmarker) rather than the deformed 3D positions.
    const s0 = { x: landmarks[i0].x * frameW, y: landmarks[i0].y * frameH };
    const s1 = { x: landmarks[i1].x * frameW, y: landmarks[i1].y * frameH };
    const s2 = { x: landmarks[i2].x * frameW, y: landmarks[i2].y * frameH };

    // Destination triangle in the bake canvas, from this mesh's own UVs —
    // canonical_face_model.obj's v-coordinate is already bottom-left origin
    // per the OBJ/UV convention, and canvas is top-left, so v is flipped.
    const d0 = { x: uvAttr.getX(i0) * BAKE_SIZE, y: (1 - uvAttr.getY(i0)) * BAKE_SIZE };
    const d1 = { x: uvAttr.getX(i1) * BAKE_SIZE, y: (1 - uvAttr.getY(i1)) * BAKE_SIZE };
    const d2 = { x: uvAttr.getX(i2) * BAKE_SIZE, y: (1 - uvAttr.getY(i2)) * BAKE_SIZE };

    drawAffineTriangle(ctx, videoFrame, s0, s1, s2, d0, d1, d2);
  }

  return canvas;
}

// Per-triangle affine warp: solves for the 2x3 matrix that carries the
// source triangle onto the destination triangle, clips to the destination
// triangle so neighbouring triangles in the same UV island do not bleed into
// each other, and draws the source image through it. This is the same
// technique canvas-based texture bakers have used since 2D <canvas> had no
// native triangle-mapped drawImage; there is no such primitive to call
// instead.
function drawAffineTriangle(ctx, image, s0, s1, s2, d0, d1, d2) {
  ctx.save();
  ctx.beginPath();
  ctx.moveTo(d0.x, d0.y);
  ctx.lineTo(d1.x, d1.y);
  ctx.lineTo(d2.x, d2.y);
  ctx.closePath();
  ctx.clip();

  // Solve M such that M * [s.x, s.y, 1] = [d.x, d.y] for each of the three
  // point pairs, then apply M as the canvas transform before drawing the
  // whole source image — the clip above keeps only the mapped triangle.
  const denom = s0.x * (s1.y - s2.y) - s1.x * (s0.y - s2.y) + s2.x * (s0.y - s1.y);
  if (Math.abs(denom) < 1e-6) {
    ctx.restore();
    return;
  }
  const a = (d0.x * (s1.y - s2.y) - d1.x * (s0.y - s2.y) + d2.x * (s0.y - s1.y)) / denom;
  const b = (d0.y * (s1.y - s2.y) - d1.y * (s0.y - s2.y) + d2.y * (s0.y - s1.y)) / denom;
  const c = (d0.x * (s2.x - s1.x) - d1.x * (s2.x - s0.x) + d2.x * (s1.x - s0.x)) / denom;
  const d = (d0.y * (s2.x - s1.x) - d1.y * (s2.x - s0.x) + d2.y * (s1.x - s0.x)) / denom;
  const e = (d0.x * (s1.x * s2.y - s2.x * s1.y) - d1.x * (s0.x * s2.y - s2.x * s0.y) + d2.x * (s0.x * s1.y - s1.x * s0.y)) / denom;
  const f = (d0.y * (s1.x * s2.y - s2.x * s1.y) - d1.y * (s0.x * s2.y - s2.x * s0.y) + d2.y * (s0.x * s1.y - s1.x * s0.y)) / denom;

  ctx.transform(a, b, c, d, e, f);
  ctx.drawImage(image, 0, 0);
  ctx.restore();
}

/* -------------------------------------------------------------------------- */
/* Stylize                                                                    */
/* -------------------------------------------------------------------------- */

// Applied to a copy of the pristine bake every time, never cumulatively —
// switching the <select> back to "None" has to actually restore the
// original bake, not a lossy approximation of it.
let bakedImageData = null;

function applyStylize(preset) {
  if (!bakedImageData || !texture) return;
  const canvas = texture.image;
  const ctx = canvas.getContext('2d');
  const working = new ImageData(
    new Uint8ClampedArray(bakedImageData.data),
    bakedImageData.width,
    bakedImageData.height
  );
  const d = working.data;

  if (preset === 'desaturate') {
    for (let i = 0; i < d.length; i += 4) {
      const luma = 0.299 * d[i] + 0.587 * d[i + 1] + 0.114 * d[i + 2];
      d[i] = d[i + 1] = d[i + 2] = luma;
    }
  } else if (preset === 'posterize') {
    const levels = 4;
    const step = 255 / (levels - 1);
    for (let i = 0; i < d.length; i += 4) {
      d[i] = Math.round(Math.round(d[i] / step) * step);
      d[i + 1] = Math.round(Math.round(d[i + 1] / step) * step);
      d[i + 2] = Math.round(Math.round(d[i + 2] / step) * step);
    }
  }

  ctx.putImageData(working, 0, 0);
  texture.needsUpdate = true;
  mesh.material.flatShading = preset === 'flat';
  mesh.material.needsUpdate = true;
  renderer.render(scene, camera);
}

/* -------------------------------------------------------------------------- */
/* Three.js scene                                                             */
/* -------------------------------------------------------------------------- */

let renderer = null;
let scene = null;
let camera = null;
let mesh = null;
let texture = null;

function setupScene(geometry) {
  scene = new THREE.Scene();
  camera = new THREE.PerspectiveCamera(35, els.stage.clientWidth / els.stage.clientHeight, 0.1, 100);

  scene.add(new THREE.HemisphereLight(0xffffff, 0x444444, 1.2));
  const key = new THREE.DirectionalLight(0xffffff, 1.5);
  key.position.set(2, 4, 3);
  scene.add(key);

  const material = new THREE.MeshStandardMaterial({ color: 0xcccccc });
  mesh = new THREE.Mesh(geometry, material);
  scene.add(mesh);

  const box = new THREE.Box3().setFromObject(mesh);
  const size = box.getSize(new THREE.Vector3());
  const center = box.getCenter(new THREE.Vector3());
  const verticalFov = THREE.MathUtils.degToRad(camera.fov);
  const distanceForHeight = size.y / 2 / Math.tan(verticalFov / 2);
  const distanceForWidth = size.x / 2 / Math.tan(verticalFov / 2) / camera.aspect;
  const distance = Math.max(distanceForHeight, distanceForWidth) * 1.6;
  camera.position.set(center.x, center.y, center.z + distance);
  camera.lookAt(center);

  renderer = new THREE.WebGLRenderer({ canvas: els.stage, antialias: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  resizeRenderer();
  window.addEventListener('resize', resizeRenderer);
  renderer.render(scene, camera);
}

function resizeRenderer() {
  const { clientWidth: w, clientHeight: h } = els.stage;
  if (!w || !h || !renderer) return;
  renderer.setSize(w, h, false);
  camera.aspect = w / h;
  camera.updateProjectionMatrix();
  renderer.render(scene, camera);
}

/* -------------------------------------------------------------------------- */
/* Capture                                                                    */
/* -------------------------------------------------------------------------- */

let faceLandmarker = null;
let canonicalGeometry = null;
let canonicalVertexCount = 0;
let capturing = false;
let hasCapture = false;

function nextFrame() {
  return new Promise((resolve) => requestAnimationFrame(resolve));
}

async function captureFront() {
  if (capturing || !faceLandmarker) return;
  capturing = true;
  els.captureFront.setAttribute('aria-disabled', 'true');
  els.status.textContent = `Hold still, front-on — averaging ${CALIBRATION_FRAMES} frames…`;

  const frameCanvas = document.createElement('canvas');
  frameCanvas.width = els.video.videoWidth;
  frameCanvas.height = els.video.videoHeight;
  const frameCtx = frameCanvas.getContext('2d');

  const sum = new Float64Array(canonicalVertexCount * 3);
  let count = 0;
  let lastLandmarks = null;

  try {
    while (count < CALIBRATION_FRAMES) {
      const result = faceLandmarker.detectForVideo(els.video, performance.now());
      const landmarks = result.faceLandmarks[0];

      if (landmarks) {
        // F04-AC02: never assume the landmark count matches the mesh's
        // vertex count just because both are 468 today. Logged once per
        // capture, and it is a hard stop rather than a silent slice — a
        // mismatch here means every following index in this file is reading
        // the wrong point.
        if (landmarks.length < canonicalVertexCount) {
          throw new Error(
            `FaceLandmarker returned ${landmarks.length} landmarks, fewer than the ` +
              `${canonicalVertexCount} canonical_face_model.obj has vertices — see F04-AC02`
          );
        }
        console.info(`Twin: landmarks ${landmarks.length}, mesh vertices ${canonicalVertexCount}`);

        for (let i = 0; i < canonicalVertexCount; i++) {
          sum[i * 3] += landmarks[i].x;
          sum[i * 3 + 1] += landmarks[i].y;
          sum[i * 3 + 2] += landmarks[i].z;
        }
        count++;
        lastLandmarks = landmarks;
        frameCtx.drawImage(els.video, 0, 0, frameCanvas.width, frameCanvas.height);
      }

      await nextFrame();
    }
  } finally {
    capturing = false;
    els.captureFront.removeAttribute('aria-disabled');
  }

  const averaged = new Array(canonicalVertexCount);
  for (let i = 0; i < canonicalVertexCount; i++) {
    averaged[i] = { x: sum[i * 3] / count, y: sum[i * 3 + 1] / count, z: sum[i * 3 + 2] / count };
  }

  deformToLandmarks(canonicalGeometry, canonicalVertexCount, averaged);
  const bakeCanvas = bakeTexture(canonicalGeometry, canonicalVertexCount, frameCanvas, lastLandmarks);

  texture = new THREE.CanvasTexture(bakeCanvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  bakedImageData = bakeCanvas.getContext('2d').getImageData(0, 0, bakeCanvas.width, bakeCanvas.height);

  mesh.material.map = texture;
  mesh.material.color.set(0xffffff);
  mesh.material.needsUpdate = true;
  applyStylize(els.stylize.value);

  hasCapture = true;
  els.download.removeAttribute('aria-disabled');
  els.status.textContent = 'Captured. Pick a style, or download as-is.';
  renderer.render(scene, camera);
}

/* -------------------------------------------------------------------------- */
/* Download                                                                   */
/* -------------------------------------------------------------------------- */

let downloading = false;

async function downloadGlb() {
  if (downloading || !hasCapture) return;
  downloading = true;
  els.download.setAttribute('aria-disabled', 'true');
  els.status.textContent = 'Building the .glb…';

  try {
    const exporter = new GLTFExporter();
    const glb = await exporter.parseAsync(mesh, { binary: true });
    const blob = new Blob([glb], { type: 'model/gltf-binary' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'twin.glb';
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
    els.status.textContent = 'Downloaded.';
  } catch (err) {
    console.error(err);
    els.status.textContent = `Could not build the download: ${err.message}`;
  } finally {
    downloading = false;
    els.download.removeAttribute('aria-disabled');
  }
}

/* -------------------------------------------------------------------------- */
/* Gate + lifecycle                                                           */
/* -------------------------------------------------------------------------- */

// Same discipline as #echo--load: aria-disabled and a guard flag, never the
// disabled property — see AGENTS.md's accessibility invariants. This button
// stays on the page through the camera permission prompt and two concurrent
// downloads (the face model, the canonical mesh).
let loading = false;

async function start() {
  if (loading) return;
  loading = true;
  els.load.setAttribute('aria-disabled', 'true');
  els.status.textContent = 'Asking for camera access…';

  let stream;
  try {
    const [cameraStream, landmarker, canonical] = await Promise.all([
      startCamera(),
      loadFaceLandmarker(),
      loadCanonicalMesh(),
    ]);
    stream = cameraStream;
    faceLandmarker = landmarker;
    canonicalGeometry = canonical.geometry;
    canonicalVertexCount = canonical.vertexCount;

    setupScene(canonicalGeometry);

    els.main.hidden = false;
    els.status.textContent = 'Running. Face the camera and press "Capture front-facing calibration".';
    els.captureFront.focus();
    els.gate.hidden = true;
  } catch (err) {
    loading = false;
    els.load.removeAttribute('aria-disabled');
    els.status.textContent = `Could not start: ${err.message}`;
    console.error(err);
    stream?.getTracks().forEach((t) => t.stop());
  }
}

function stop() {
  faceLandmarker?.close();
  faceLandmarker = null;

  els.video.srcObject?.getTracks().forEach((t) => t.stop());
  els.video.srcObject = null;

  window.removeEventListener('resize', resizeRenderer);
  renderer?.dispose();
  renderer = null;
  scene = null;
  camera = null;
  mesh = null;
  texture = null;
  bakedImageData = null;
  canonicalGeometry = null;
  canonicalVertexCount = 0;
  hasCapture = false;

  els.stylize.value = 'none';
  els.download.setAttribute('aria-disabled', 'true');

  els.main.hidden = true;
  els.gate.hidden = false;
  els.status.textContent = 'Stopped. The camera has been released.';
  loading = false;
  els.load.removeAttribute('aria-disabled');
  els.load.focus();
}

els.download.setAttribute('aria-disabled', 'true');
els.gate.hidden = false;
els.load.addEventListener('click', start);
els.captureFront.addEventListener('click', captureFront);
els.stylize.addEventListener('change', () => applyStylize(els.stylize.value));
els.download.addEventListener('click', downloadGlb);
els.stop.addEventListener('click', stop);
