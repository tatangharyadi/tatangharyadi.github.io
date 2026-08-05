// Echo: the browser half.
//
// Same shape as js/game.js on purpose — see specs/F03_MOCAP.md. This file is a
// boundary and a renderer: it asks the browser for a camera, asks LiteRT.js to
// run a pose model against each frame, and asks Three.js to draw a character.
// It does not decide what a landmark means to a bone; that rule lives in
// js/mocap-retarget.js, argued separately because it is a rule and this file
// is not supposed to have any.
//
// Nothing captured here is sent anywhere. There is no endpoint this page
// talks to at all — the model, the runtime and the character are all fetched
// same-origin, once, and every frame after that is inference against
// WebAssembly already sitting in this tab. That is the same structural
// privacy property js/ask.js has, for the same reason: the thing that would
// need to be uploaded for a server to help is exactly the thing a server is
// never sent.

import { loadLiteRt, unloadLiteRt, loadAndCompile, Tensor } from '../vendor/litert/litert-core.mjs';
import * as THREE from 'three';
import { GLTFLoader } from '../vendor/three/examples/jsm/loaders/GLTFLoader.js';
import { buildBoneMap, buildRestDirections, levelHead, retarget } from './mocap-retarget.js';

// A directory, not a file. loadLiteRt() picks between
// litert_wasm_compat_internal.js and litert_wasm_internal.js itself, by
// feature-detecting relaxed SIMD — see the note below on why it is never
// asked to consider the threaded or JSPI builds.
const LITERT_DIR = 'vendor/litert/';
const POSE_MODEL_URL = 'assets/models/pose-landmark-full/pose_landmark_full.tflite';
const CHARACTER_URL = 'assets/character/RobotExpressive.glb';

const els = {
  gate: document.getElementById('echo--gate'),
  load: document.getElementById('echo--load'),
  status: document.getElementById('echo--status'),
  main: document.getElementById('echo'),
  video: document.getElementById('echo--video'),
  stage: document.getElementById('echo--stage'),
  stop: document.getElementById('echo--stop'),
};

/* -------------------------------------------------------------------------- */
/* Camera + model loading                                                     */
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

// No options argument, and that omission is load-bearing rather than an
// oversight. GitHub Pages cannot set the COOP/COEP response headers cross-
// origin isolation needs, so self.crossOriginIsolated is false here the same
// way it is in js/ask.js, and SharedArrayBuffer does not exist. Passing
// `{ threads: n }` or `{ jspi: true }` would ask loadLiteRt to consider a
// build that needs exactly the isolation this origin cannot grant, and it
// would fail at runtime rather than fall back. Leaving options undefined
// restricts the choice, internally, to the two non-threaded builds
// (litert_wasm_compat_internal / litert_wasm_internal) — read
// vendor/litert/litert-core.mjs's own `load()` if this ever needs re-checking.
async function loadPoseModel() {
  await loadLiteRt(LITERT_DIR);
  const model = await loadAndCompile(POSE_MODEL_URL);

  // Logged once rather than hardcoded: the model's real input/output shapes
  // and names are read from the model itself below, at every call site that
  // needs them, specifically so a future change to the .tflite file does not
  // require a matching hand-edit here. This line is the one place a human
  // can go check that reading against the console.
  console.info('Echo: pose model inputs', model.getInputDetails());
  console.info('Echo: pose model outputs', model.getOutputDetails());
  return model;
}

async function loadCharacter() {
  const loader = new GLTFLoader();
  const gltf = await loader.loadAsync(CHARACTER_URL);
  return gltf.scene;
}

/* -------------------------------------------------------------------------- */
/* Frame -> Tensor                                                            */
/* -------------------------------------------------------------------------- */

// An offscreen canvas the size of whatever the model's own input tensor says
// it wants, not a fixed constant: read once from getInputDetails() after the
// model loads, in prepareFrameCanvas() below, rather than assumed here.
let frameCanvas = null;
let frameCtx = null;

function prepareFrameCanvas(inputDetails) {
  // Input layout is [batch, height, width, channels] (NHWC), which is what
  // every MediaPipe/LiteRT vision model published so far uses. shape is an
  // Int32Array; batch is always 1 for a single-frame call.
  const [, height, width] = inputDetails.shape;
  frameCanvas = document.createElement('canvas');
  frameCanvas.width = width;
  frameCanvas.height = height;
  frameCtx = frameCanvas.getContext('2d', { willReadFrequently: true });
}

// Draws the current video frame into the model's input size and returns a
// Tensor built from it via Tensor.fromTypedArray — the only host-memory
// construction path litert-core.mjs exposes; there is no fromTexture/
// fromCanvas convenience to reach for instead.
function videoFrameToTensor(video, inputDetails) {
  if (inputDetails.dtype !== 'float32') {
    // A wrong assumption here should throw, not silently feed the model
    // bytes it will misinterpret — see the console.info in loadPoseModel().
    throw new Error(`pose model expects input dtype float32, got ${inputDetails.dtype}`);
  }

  frameCtx.drawImage(video, 0, 0, frameCanvas.width, frameCanvas.height);
  const { data } = frameCtx.getImageData(0, 0, frameCanvas.width, frameCanvas.height);

  const pixelCount = frameCanvas.width * frameCanvas.height;
  const rgb = new Float32Array(pixelCount * 3);
  for (let i = 0; i < pixelCount; i++) {
    // getImageData is always RGBA; the model's channel count (inputDetails
    // shape's last dimension) is 3, so alpha is dropped here rather than
    // asking the model to accept a channel it did not declare.
    rgb[i * 3] = data[i * 4] / 255;
    rgb[i * 3 + 1] = data[i * 4 + 1] / 255;
    rgb[i * 3 + 2] = data[i * 4 + 2] / 255;
  }

  return Tensor.fromTypedArray(rgb, inputDetails.shape);
}

// pose_landmark_full.tflite's actual "Identity" output (read via the
// console.info in loadPoseModel(), not assumed) is 39 landmarks of
// (x, y, z, visibility, presence): the 33 published BlazePose body points
// plus 6 auxiliary ROI-tracking points MediaPipe appends after them, at
// indices 33-38. mocap-retarget.js only ever reads indices up to 28, so the
// auxiliary points are carried along and simply never read. Two of the
// model's other four outputs — world landmarks (39 x 3, no visibility) and a
// lone presence scalar — describe the same 39 points at a different stride,
// which is exactly why this file matches on the full flat length below
// rather than assuming which output index holds it.
const LANDMARK_COUNT = 39;
const LANDMARK_STRIDE = 5;

// The raw "Identity" tensor carries visibility as a pre-sigmoid logit, not a
// [0, 1] probability — MediaPipe's own graph applies this same activation in
// a separate TensorsToLandmarksCalculator step that is not baked into the
// .tflite file. Confirmed empirically: logged raw values ranged well outside
// [0, 1] (e.g. 8.01, -1.49, 0.13). mocap-retarget.js's MIN_VISIBILITY compares
// against a probability, so the logit has to be squashed here first.
function sigmoid(x) {
  return 1 / (1 + Math.exp(-x));
}

function tensorDataToLandmarks(flat) {
  if (flat.length < LANDMARK_COUNT * LANDMARK_STRIDE) {
    throw new Error(
      `pose output has ${flat.length} values, fewer than the ${LANDMARK_COUNT * LANDMARK_STRIDE} ` +
        `this file assumes (${LANDMARK_COUNT} landmarks x ${LANDMARK_STRIDE}) — see the note above tensorDataToLandmarks`
    );
  }
  const landmarks = new Array(LANDMARK_COUNT);
  for (let i = 0; i < LANDMARK_COUNT; i++) {
    const o = i * LANDMARK_STRIDE;
    landmarks[i] = { x: flat[o], y: flat[o + 1], z: flat[o + 2], visibility: sigmoid(flat[o + 3]) };
  }
  return landmarks;
}

/* -------------------------------------------------------------------------- */
/* Three.js scene                                                             */
/* -------------------------------------------------------------------------- */

let renderer = null;
let scene = null;
let camera = null;
let character = null;
let boneMap = null;
let restDirections = null;

function setupScene(characterRoot) {
  scene = new THREE.Scene();

  camera = new THREE.PerspectiveCamera(35, els.stage.clientWidth / els.stage.clientHeight, 0.1, 100);

  scene.add(new THREE.HemisphereLight(0xffffff, 0x444444, 1.2));
  const key = new THREE.DirectionalLight(0xffffff, 1.5);
  key.position.set(2, 4, 3);
  scene.add(key);

  character = characterRoot;
  scene.add(character);
  boneMap = buildBoneMap(character);
  restDirections = buildRestDirections(boneMap);
  levelHead(boneMap);

  // Framed from the character's own measured size, not a hardcoded distance:
  // an assumed ~1.7-unit-tall figure put the camera inside RobotExpressive's
  // actual rest-pose bounds (roughly 4.8 units top to bottom), producing an
  // extreme close-up of its hip rather than a body shot. Fitting to the
  // real box means a future character swap frames correctly with no matching
  // hand-edit here, the same reasoning tensorDataToLandmarks above uses for
  // reading shapes off the model rather than assuming them.
  const box = new THREE.Box3().setFromObject(character);
  const size = box.getSize(new THREE.Vector3());
  const center = box.getCenter(new THREE.Vector3());
  const verticalFov = THREE.MathUtils.degToRad(camera.fov);
  const distanceForHeight = size.y / 2 / Math.tan(verticalFov / 2);
  const distanceForWidth = size.x / 2 / Math.tan(verticalFov / 2) / camera.aspect;
  const distance = Math.max(distanceForHeight, distanceForWidth) * 1.3;
  camera.position.set(center.x, center.y, center.z + distance);
  camera.lookAt(center);

  renderer = new THREE.WebGLRenderer({ canvas: els.stage, antialias: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  resizeRenderer();
  window.addEventListener('resize', resizeRenderer);
}

function resizeRenderer() {
  const { clientWidth: w, clientHeight: h } = els.stage;
  if (!w || !h || !renderer) return;
  renderer.setSize(w, h, false);
  camera.aspect = w / h;
  camera.updateProjectionMatrix();
}

const retargetScratch = {};

function renderFrame(landmarks) {
  retarget(landmarks, boneMap, restDirections, THREE, retargetScratch);
  renderer.render(scene, camera);
}

/* -------------------------------------------------------------------------- */
/* Inference loop                                                             */
/* -------------------------------------------------------------------------- */

let stream = null;
let model = null;
let rafHandle = null;
let running = false;

// Awaits its own inference before scheduling the next frame, rather than
// firing model.run() and immediately re-arming requestAnimationFrame.
// BlazePose full does not finish inside one 16ms frame budget, so the
// un-awaited version would queue inference calls faster than they resolve
// and allocate a fresh input Tensor every 16ms regardless — an unbounded
// backlog of both. `running` (not just rafHandle) gates every step so a
// stop() mid-inference is honoured as soon as the in-flight call returns,
// rather than rendering one more frame into a canvas stop() is tearing down.
// stop() needs to know when the in-flight iteration below has actually
// finished, not just that `running` has been set to false: model.run() and
// output.data() are both awaits into vendor/litert/litert-core.mjs, and
// unloadLiteRt() tearing down the global LiteRt environment while one of
// those is still pending corrupts it for the *next* start() rather than
// just failing this one. loopPromise is that handle.
let loopPromise = null;

function loop() {
  if (!running) return;
  loopPromise = runLoopIteration().finally(() => {
    loopPromise = null;
  });
}

async function runLoopIteration() {
  const inputDetails = model.getInputDetails()[0];
  const tensor = videoFrameToTensor(els.video, inputDetails);
  let result;
  try {
    result = await model.run(tensor);
  } catch (err) {
    console.error('Echo: inference frame failed', err);
    result = null;
  } finally {
    tensor.delete();
  }

  if (result) {
    // model.run() was given a single Tensor, so per litert-core.mjs this is
    // always the array branch, never the keyed-record one. BlazePose full
    // has several outputs (landmarks, world landmarks, presence, a
    // segmentation mask); which index holds the flat landmark array is only
    // knowable by its length, not its position — see the console.info
    // logging in loadPoseModel() if this ever needs re-deriving. Every
    // output is read and deleted regardless, so nothing here leaks WASM
    // heap even for the outputs this file does not use.
    const outputs = Array.isArray(result) ? result : Object.values(result);
    try {
      let landmarks = null;
      for (const output of outputs) {
        const flat = await output.data();
        if (!landmarks && flat.length === LANDMARK_COUNT * LANDMARK_STRIDE) {
          landmarks = tensorDataToLandmarks(flat);
        }
      }
      if (landmarks && running) renderFrame(landmarks);
    } finally {
      for (const output of outputs) output.delete?.();
    }
  }

  if (running) rafHandle = requestAnimationFrame(() => loop());
}

/* -------------------------------------------------------------------------- */
/* Gate + lifecycle                                                           */
/* -------------------------------------------------------------------------- */

// Same discipline as #ask--load: aria-disabled and a guard flag, never the
// disabled property. This button stays on the page through the camera
// permission prompt and two concurrent downloads (the pose model, the
// character), and disabling it on press would strand a keyboard user on
// <body> for that entire window — see AGENTS.md's accessibility invariants.
let loading = false;

async function start() {
  if (loading) return;
  loading = true;
  els.load.setAttribute('aria-disabled', 'true');
  els.status.textContent = 'Asking for camera access…';

  try {
    const [cameraStream, poseModel, characterRoot] = await Promise.all([
      startCamera(),
      loadPoseModel(),
      loadCharacter(),
    ]);
    stream = cameraStream;
    model = poseModel;

    prepareFrameCanvas(model.getInputDetails()[0]);
    setupScene(characterRoot);

    // Focus the destination before hiding the origin, same order js/ask.js
    // uses for its own gate: an element must still be in the accessibility
    // tree to receive focus, so hiding #echo--gate first would drop focus to
    // <body> for an instant rather than moving it straight to #echo--stop.
    els.main.hidden = false;
    els.status.textContent = 'Running. Step back until your shoulders, hips and knees are all in frame.';
    els.stop.focus();
    els.gate.hidden = true;

    running = true;
    loop();
  } catch (err) {
    loading = false;
    els.load.removeAttribute('aria-disabled');
    els.status.textContent = `Could not start: ${err.message}`;
    console.error(err);
    stream?.getTracks().forEach((t) => t.stop());
  }
}

async function stop() {
  // Tears down everything start() creates, not just the camera, so a second
  // press of #echo--load is a clean run rather than a stacked one:
  // loadLiteRt() throws if a runtime is already loaded, and a second
  // WebGLRenderer on the same <canvas> would fight the first for the GL
  // context. unloadLiteRt() is LiteRT's own reset for exactly this case.
  running = false;
  if (rafHandle !== null) cancelAnimationFrame(rafHandle);
  rafHandle = null;

  // running=false only stops the *next* iteration from being scheduled; an
  // iteration already inside model.run()/output.data() is still pending
  // WASM work in vendor/litert/litert-core.mjs. unloadLiteRt() below tears
  // down the global LiteRt environment those calls read from, so it must
  // wait for that iteration to actually finish rather than race it — racing
  // it once left the environment in a state where the next start()'s
  // model.getInputDetails() threw.
  if (loopPromise) await loopPromise.catch(() => {});

  stream?.getTracks().forEach((t) => t.stop());
  stream = null;
  window.removeEventListener('resize', resizeRenderer);

  // dispose() alone frees the GPU resources this renderer held. It does not
  // touch the WebGL context itself, which is deliberate: a canvas element can
  // only ever be given one WebGL context for its whole lifetime, so the next
  // start() creates a WebGLRenderer on this same #echo--stage canvas.
  // forceContextLoss() was here too until it broke exactly that restart —
  // it marks the context lost rather than freeing it, getContext() on this
  // canvas keeps handing back that same dead context forever after (no
  // 'webglcontextrestored' ever fires without a real GPU-driver loss), and
  // the next WebGLRenderer's precision probe (gl.getShaderPrecisionFormat)
  // returns null on a lost context, which is the TypeError this replaced.
  renderer?.dispose();
  renderer = null;
  scene = null;
  camera = null;
  character = null;
  boneMap = null;
  restDirections = null;
  model = null;
  unloadLiteRt();

  els.main.hidden = true;
  els.gate.hidden = false;
  els.status.textContent = 'Stopped. The camera has been released.';
  loading = false;
  els.load.removeAttribute('aria-disabled');
  els.load.focus();
}

els.gate.hidden = false;
els.load.addEventListener('click', start);
els.stop.addEventListener('click', stop);
