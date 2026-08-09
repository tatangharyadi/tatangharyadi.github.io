// Iris: a Breakout-style game whose paddle follows the visitor's gaze.
//
// The boundary shape below — start camera, load FaceLandmarker, run
// detectForVideo() every frame, stop and release everything on demand — is
// the same one js/twin.js uses, reusing Twin's already-vendored runtime and
// already-committed model (see specs/F05_IRIS.md, F05-AC03). What differs
// is what happens with the model's output: Twin deforms a mesh with the 468
// face-mesh points; Iris reads the model's face-blendshapes output (a
// classifier already bundled inside the same face_landmarker.task file,
// enabled here with outputFaceBlendshapes: true, that Twin's own code
// leaves off) to drive a paddle, and never touches the DOM with a 3D
// renderer at all — the "renderer" here is a plain 2D canvas. An earlier
// version of this file read the ten raw iris landmarks (indices 468-477)
// directly; specs/F05_IRIS.md documents why that signal turned out to
// carry no usable gaze information on real hardware.
//
// Two accuracy techniques ported from public MediaPipe-based gaze trackers
// (see specs/F05_IRIS.md's "Head-pose correction and multi-point
// calibration" section) sit on top of the blendshape signal: a head-yaw
// correction term (also read from face_landmarker.task, via
// outputFacialTransformationMatrixes: true) and a five-point weighted
// polynomial calibration fit in place of the original two-point linear map.
// Both are first-guess ports pending a real-hardware session, same
// discipline the rest of this file's constants already follow.

import { FaceLandmarker, FilesetResolver } from '../vendor/mediapipe/tasks-vision/vision_bundle.mjs';

const WASM_BASE = 'vendor/mediapipe/tasks-vision/wasm';
const FACE_MODEL_URL = 'assets/models/face-landmarker/face_landmarker.task';

// Three straight rounds of hand-rolled geometry on the 478 face-mesh points
// (raw iris x, then iris-x normalized against the eye's own corner span,
// then that same ratio with heavier filtering) all failed on real hardware.
// The last round's ?debug=1 readout proved why: smoothedGazeX didn't move
// at all between a deliberate hard-left hold and a deliberate hard-right
// hold. The corner-ratio signal carried no gaze information to filter or
// calibrate in the first place — see specs/F05_IRIS.md's "the raw signal
// itself has no dynamic range" section.
//
// The committed model bundle (assets/models/face-landmarker/face_landmarker.task)
// already contains a face_blendshapes.tflite sub-model — a classifier
// trained specifically to estimate expression and gaze-direction strength,
// including the ARKit-standard eyeLookInLeft/eyeLookOutLeft/eyeLookInRight/
// eyeLookOutRight categories used below. This is a purpose-built signal for
// exactly this problem, not geometry we derived ourselves, and needs no new
// asset: outputFaceBlendshapes: true is the only change to what the model
// loads.
const RAW_GAZE_MEDIAN_WINDOW = 5;
const IRIS_X_EMA_ALPHA = 0.15;

// Public reference implementations that report working accuracy on the same
// FaceLandmarker API (github.com/aciderix/React-Eye-Tracker-V1,
// github.com/ChiShengChen/gaze_track_webcam) both do two things this file
// didn't do before: correct for head pose separately from the eye signal,
// and fit a multi-point regression through calibration samples instead of a
// two-point linear map. Neither claim has been checked against this file's
// own real-hardware session yet — both are first-guess ports pending that,
// same discipline as MIRROR_GAZE_X and IRIS_X_EMA_ALPHA above.

// Blendshape scores are computed from landmarks already aligned to a
// canonical face by the model, so they're less head-pose-sensitive than the
// raw corner-ratio geometry the earlier, abandoned signal used — but ARKit
// blendshape classifiers are documented to still drift under head rotation,
// not to be pose-invariant outright. facialTransformationMatrixes is the
// same already-committed model's own head-pose output (no new asset,
// exactly the "flip a flag on the call Twin already makes" pattern
// outputFaceBlendshapes used), so head yaw is available for free rather
// than estimated from landmarks a second way.
const HEAD_YAW_CORRECTION_GAIN = 0.2;

// CAL_POINTS replaces the old two-point (left, right) capture with five,
// matching the shape of both reference implementations' multi-point
// calibration (9-point Ridge regression; polynomial regression with edge
// weighting) rather than their exact point count — Iris only drives a 1D
// paddle position, not a 2D screen cursor, so five points spanning the
// track is proportionate, not a corner cut.
const CAL_POINTS = [
    { target: 0, label: 'the far left edge of your screen' },
    { target: 0.25, label: 'a point a quarter of the way from the left' },
    { target: 0.5, label: 'the center of your screen' },
    { target: 0.75, label: 'a point a quarter of the way from the right' },
    { target: 1, label: 'the far right edge of your screen' },
];

// Fit degree, not point count: five points into a degree-2 polynomial is
// overdetermined (5 equations, 3 unknowns), which is what lets the least-
// squares fit below smooth across noisy individual captures instead of
// passing through each one exactly, the same role gaze_track_webcam's
// polynomial regression plays over its own calibration grid.
const CAL_POLY_DEGREE = 2;

// gaze_track_webcam's "edge weighting" downweights the interior points
// relative to the extremes so the fit favors getting the paddle's full
// left/right reach right over interior smoothness — the paddle running out
// of track before the visitor's own comfortable gaze range does is a more
// visible failure than a slightly uneven middle.
const CAL_EDGE_WEIGHT = 2;
const CAL_INTERIOR_WEIGHT = 1;

// The calibration fit amplifies noise (see CAPTURE_AVERAGE_WINDOW's comment)
// in proportion to how much it stretches the raw range — smoothing the raw
// signal further can't remove that amplification, since the amplification
// happens after smoothedGazeX is already computed. A second EMA stage on
// calibratedX()'s own output, applied where the amplification actually
// happens, directly damps the jitter a visitor sees in the paddle without
// adding the extra latency heavier upstream smoothing would cost the raw
// signal. Same first-guess discipline as IRIS_X_EMA_ALPHA.
const PADDLE_EMA_ALPHA = 0.2;

// Unlike raw landmark.x (camera-frame-relative, needed the flip the old
// corner-ratio code applied here), the blendshape categories are already
// subject-relative — "Right" means the visitor's own right eye regardless
// of how the camera image is oriented. gazeScoreFromBlendshapes() already
// resolves to "higher rawX = visitor looked to their own right", which is
// the direction the paddle should move, so no flip should be needed. Kept
// as a named toggle rather than deleted because the previous round's
// verified assumption ("needs a flip") just inverted; if a real session
// says the paddle now moves backwards, flip this rather than re-deriving
// the sign from scratch again.
const MIRROR_GAZE_X = false;

// Below this spread across all five captured raw values, calibration treats
// the whole session as noise rather than a real range — smoothedGazeX is
// already EMA-smoothed, so a visitor's five captures all landing this close
// together are almost certainly a signal that isn't moving (a static or
// stuck camera feed hits this by construction) rather than five genuinely
// distinct gaze positions.
const CAL_MIN_SEPARATION = 0.03;

// A real-hardware session reported both "barely moves" and "jittery" —
// together, the signature of a calibration fit stretching a small raw range
// to fill the whole track, which multiplies whatever per-frame noise sits in
// that raw range by the same factor. A single instantaneous smoothedGazeX
// read at the moment "Capture this point" is clicked is exactly the kind of
// value that noise shows up in most: the EMA above needs several frames to
// settle after a glance, and a visitor plausibly glances then clicks before
// it has. Averaging the last CAPTURE_AVERAGE_WINDOW frames' readings instead
// gives each calibration sample a chance to reflect where the gaze signal
// actually settled, not whatever it was mid-transition — first-guess like
// every other constant here, but addressing the reported symptom directly
// rather than adjusting an unrelated knob again.
const CAPTURE_AVERAGE_WINDOW = 20;

const PADDLE_WIDTH = 90;
const PADDLE_HEIGHT = 12;
const PADDLE_Y_MARGIN = 18;
const BALL_RADIUS = 6;
const BRICK_ROWS = 5;
const BRICK_COLS = 8;
const BRICK_HEIGHT = 18;
const BRICK_GAP = 4;
const BRICK_TOP_MARGIN = 36;
const LIVES_START = 3;

const SPEECH_LINES = {
    start: ['Look around to find the paddle.'],
    brickStreak: ["Nice run.", "Keep going.", "Good streak."],
    levelClear: ["Board clear.", "On to the next one."],
    lifeLost: ["Lost one.", "Ball's back."],
    gameOver: ["Game over.", "That's the game."],
};

const els = {
    gate: document.getElementById('iris--gate'),
    load: document.getElementById('iris--load'),
    status: document.getElementById('iris--status'),
    calibrate: document.getElementById('iris--calibrate'),
    calInstructions: document.getElementById('iris--cal-instructions'),
    calMarker: document.getElementById('iris--cal-marker'),
    calTarget: document.getElementById('iris--cal-target'),
    calCapture: document.getElementById('iris--cal-capture'),
    calSkip: document.getElementById('iris--cal-skip'),
    main: document.getElementById('iris'),
    video: document.getElementById('iris--video'),
    stage: document.getElementById('iris--stage'),
    recalibrate: document.getElementById('iris--recalibrate'),
    stop: document.getElementById('iris--stop'),
    debug: document.getElementById('iris--debug'),
};

// ?debug=1 shows the raw per-eye signal on screen instead of just the
// paddle it drives. Two "fix" commits (corner-relative normalization, then
// noise filtering) have both been reported as not working against real
// hardware, and the sandbox's fake camera never moves — it cannot exhibit
// the failure either before or after a fix, so it cannot tell us which
// theory is right. This readout is how a real webcam session tells us
// instead of another blind guess at a filter constant.
const DEBUG = new URLSearchParams(window.location.search).get('debug') === '1';
if (DEBUG) els.debug.hidden = false;
let droppedFrames = 0;

const ctx = els.stage.getContext('2d');

let loading = false;
let stream = null;
let faceLandmarker = null;
let rafId = null;
let smoothedGazeX = 0.5;
let voice = null;

// 'idle' | 'calibrating' | 'playing'. The rAF loop in frame() keeps reading
// the camera and updating smoothedGazeX in every mode once it starts —
// only what frame() does with that value (move the live marker vs. step
// the game) depends on this.
let mode = 'idle';

// One {raw, target} sample per CAL_POINTS entry, pushed as the visitor
// steps through calibration; calStepIndex is which point comes next.
// calCoeffs holds the fitted polynomial (see fitPolynomial()) that
// calibratedX() evaluates; null means uncalibrated (skipped or not yet
// finished), which is the pre-calibration behaviour: smoothedGazeX used
// directly as the 0..1 fraction of the paddle track.
let calStepIndex = 0;
let calSamples = [];
let calCoeffs = null;

// Last CAPTURE_AVERAGE_WINDOW smoothedGazeX readings, oldest first, so
// captureCalPoint() can average instead of reading a single frame that might
// mid-transition. Separate from rawGazeHistory, which feeds the median
// filter upstream of smoothedGazeX — this buffer is downstream of it.
let recentSmoothed = [];

// Paddle-space smoothing, downstream of calibratedX() rather than of
// smoothedGazeX — see PADDLE_EMA_ALPHA's comment for why this needs to be a
// separate stage. null until the first frame so stepGame() can seed it
// directly instead of easing in from an arbitrary starting value.
let smoothedPaddleFraction = null;

// Latest per-frame head yaw (radians, first-guess sign convention — see
// yawFromMatrix()) and the correction applied, kept for ?debug=1 only; the
// correction itself is applied inline in updateGazeFromResult().
let lastYaw = 0;

let game = null; // set up fresh each beginGame()

// Cached --text/--accent-text/--border/--bg-alt, read once here rather than
// every drawGame() call, and refreshed on a scheme change below. Read at
// module scope, not inside start(), so drawGame() never sees a null theme
// regardless of call order.
let theme = null;

function readTheme() {
    const style = getComputedStyle(document.documentElement);
    theme = {
        text: style.getPropertyValue('--text').trim(),
        accent: style.getPropertyValue('--accent-text').trim(),
        border: style.getPropertyValue('--border').trim(),
        bgAlt: style.getPropertyValue('--bg-alt').trim(),
    };
}

readTheme();

function setStatus(message) {
    els.status.textContent = message;
}

function pickVoice() {
    const voices = window.speechSynthesis ? window.speechSynthesis.getVoices() : [];
    if (!voices.length) return null;
    const score = (v) => {
        let s = 0;
        if (v.lang && v.lang.toLowerCase().startsWith('en')) s += 2;
        if (v.localService) s += 2;
        if (v.default) s += 1;
        return s;
    };
    return voices.slice().sort((a, b) => score(b) - score(a))[0];
}

function speak(line) {
    if (!window.speechSynthesis) return;
    const utterance = new SpeechSynthesisUtterance(line);
    if (voice) utterance.voice = voice;
    utterance.rate = 1.05;
    window.speechSynthesis.speak(utterance);
}

function say(lines) {
    const line = lines[Math.floor(Math.random() * lines.length)];
    speak(line);
}

async function startCamera() {
    const s = await navigator.mediaDevices.getUserMedia({
        video: { facingMode: 'user', width: { ideal: 640 }, height: { ideal: 480 } },
        audio: false,
    });
    els.video.srcObject = s;
    await els.video.play();
    return s;
}

async function loadFaceLandmarker() {
    const filesetResolver = await FilesetResolver.forVisionTasks(WASM_BASE);
    return FaceLandmarker.createFromOptions(filesetResolver, {
        baseOptions: { modelAssetPath: FACE_MODEL_URL },
        runningMode: 'VIDEO',
        numFaces: 1,
        outputFaceBlendshapes: true,
        outputFacialTransformationMatrixes: true,
    });
}

function newGame() {
    const width = els.stage.width;
    const height = els.stage.height;

    const bricks = [];
    const brickWidth = (width - BRICK_GAP * (BRICK_COLS + 1)) / BRICK_COLS;
    for (let row = 0; row < BRICK_ROWS; row++) {
        for (let col = 0; col < BRICK_COLS; col++) {
            bricks.push({
                x: BRICK_GAP + col * (brickWidth + BRICK_GAP),
                y: BRICK_TOP_MARGIN + row * (BRICK_HEIGHT + BRICK_GAP),
                w: brickWidth,
                h: BRICK_HEIGHT,
                alive: true,
            });
        }
    }

    return {
        width,
        height,
        brickWidth,
        bricks,
        paddleX: width / 2 - PADDLE_WIDTH / 2,
        ball: { x: width / 2, y: height - 60, vx: 140, vy: -140 },
        lives: LIVES_START,
        score: 0,
        brickStreak: 0,
        over: false,
        outcome: null, // 'cleared' | 'lost'
        lastTime: null,
    };
}

function resetBall(g) {
    g.ball.x = g.width / 2;
    g.ball.y = g.height - 60;
    g.ball.vx = 140 * (Math.random() < 0.5 ? -1 : 1);
    g.ball.vy = -140;
}

function medianOf(values) {
    const sorted = values.slice().sort((a, b) => a - b);
    return sorted[Math.floor(sorted.length / 2)];
}

// Recent raw (pre-EMA) readings, oldest first, capped at
// RAW_GAZE_MEDIAN_WINDOW — see the constant's comment for why this exists.
let rawGazeHistory = [];

// Looking to one side rotates each eye a different way relative to its own
// nose-side/temple-side axis (this is normal conjugate gaze, not an error):
// looking right moves the right eye temporally (eyeLookOutRight) and the
// left eye nasally (eyeLookInLeft); looking left is the mirror pair. Adding
// each side's two categories, rather than trusting either eye alone,
// halves the effect of one eye's blendshape score being noisier than the
// other's on a given frame.
function gazeScoreFromBlendshapes(categories) {
    const score = {};
    for (const c of categories) score[c.categoryName] = c.score;
    const right = ((score.eyeLookOutRight || 0) + (score.eyeLookInLeft || 0)) / 2;
    const left = ((score.eyeLookInRight || 0) + (score.eyeLookOutLeft || 0)) / 2;
    return { right, left, score };
}

// Column-major 4x4 model-to-camera transform, same convention
// FaceLandmarker documents for facialTransformationMatrixes: R[row][col] =
// data[col*4+row]. Yaw (rotation about the vertical axis) from a rotation
// matrix's R[0][2] and R[2][2] terms is a standard extraction, but the sign
// this resolves to relative to "visitor turned their head to their own
// right" has not been checked against a real session — first-guess, like
// every other constant in this file's head-pose path.
function yawFromMatrix(matrixData) {
    return Math.atan2(matrixData[8], matrixData[10]);
}

function updateGazeFromResult(result) {
    const blendshapes = result.faceBlendshapes && result.faceBlendshapes[0];
    if (!blendshapes) {
        droppedFrames += 1;
        if (DEBUG) renderDebug({ right: null, left: null, rawX: null, medianX: null });
        return; // no face this frame; hold the last position rather than guess
    }

    const matrix = result.facialTransformationMatrixes && result.facialTransformationMatrixes[0];
    lastYaw = matrix ? yawFromMatrix(matrix.data) : 0;

    const { right, left, score } = gazeScoreFromBlendshapes(blendshapes.categories);
    // right/left are each 0..1 confidences, not a position — center on 0.5
    // and let a stronger "look right" than "look left" (or vice versa) push
    // away from it in either direction.
    const eyeX = 0.5 + (right - left) / 2;
    // Subtracting a yaw-scaled term compensates for head rotation reading as
    // eye movement — turning the head right, without moving the eyes in
    // their sockets, should not also read as "looking right". HEAD_YAW_CORRECTION_GAIN
    // is unverified against real hardware; see the constant's comment.
    const rawX = Math.min(1, Math.max(0, eyeX - HEAD_YAW_CORRECTION_GAIN * lastYaw));
    rawGazeHistory.push(rawX);
    if (rawGazeHistory.length > RAW_GAZE_MEDIAN_WINDOW) rawGazeHistory.shift();
    const medianX = medianOf(rawGazeHistory);

    const gazeX = MIRROR_GAZE_X ? 1 - medianX : medianX;
    smoothedGazeX = smoothedGazeX + IRIS_X_EMA_ALPHA * (gazeX - smoothedGazeX);

    recentSmoothed.push(smoothedGazeX);
    if (recentSmoothed.length > CAPTURE_AVERAGE_WINDOW) recentSmoothed.shift();

    if (DEBUG) renderDebug({ right, left, rawX, medianX, score });
}

function fmt(n) {
    return n === null || n === undefined ? ' -- ' : n.toFixed(3);
}

function renderDebug(f) {
    const s = f.score || {};
    const coeffsStr = calCoeffs ? calCoeffs.map((c) => c.toFixed(3)).join(', ') : ' -- ';
    els.debug.textContent =
        `eyeLookOutRight ${fmt(s.eyeLookOutRight)}  eyeLookInLeft  ${fmt(s.eyeLookInLeft)}  -> right ${fmt(f.right)}\n` +
        `eyeLookInRight  ${fmt(s.eyeLookInRight)}  eyeLookOutLeft ${fmt(s.eyeLookOutLeft)}  -> left  ${fmt(f.left)}\n` +
        `head yaw (rad) ${fmt(lastYaw)}  correction ${fmt(-HEAD_YAW_CORRECTION_GAIN * lastYaw)}\n` +
        `raw ${fmt(f.rawX)}  median ${fmt(f.medianX)}  smoothed ${fmt(smoothedGazeX)}\n` +
        `calibration fit (c0..c${CAL_POLY_DEGREE}): ${coeffsStr}\n` +
        `paddle fraction (target/smoothed) ${fmt(calibratedX(smoothedGazeX))} / ${fmt(smoothedPaddleFraction)}\n` +
        `dropped frames (no face this frame): ${droppedFrames}`;
}

// Least-squares fit of a degree-N polynomial y = c0 + c1*x + c2*x^2 + ...
// through weighted (raw, target) samples, via the normal equations solved
// by Gaussian elimination with partial pivoting. Small and dependency-free
// on purpose: five points and a handful of unknowns don't need a linear
// algebra library, the same "a few lines of CSS/JS" bar AGENTS.md holds the
// rest of this site to.
function fitPolynomial(samples, degree, weightOf) {
    const cols = degree + 1;
    const AtA = Array.from({ length: cols }, () => new Array(cols).fill(0));
    const Atb = new Array(cols).fill(0);

    for (const sample of samples) {
        const w = weightOf(sample);
        const powers = new Array(cols);
        let p = 1;
        for (let k = 0; k < cols; k++) {
            powers[k] = p;
            p *= sample.raw;
        }
        for (let i = 0; i < cols; i++) {
            Atb[i] += w * powers[i] * sample.target;
            for (let j = 0; j < cols; j++) {
                AtA[i][j] += w * powers[i] * powers[j];
            }
        }
    }

    return solveLinearSystem(AtA, Atb);
}

function solveLinearSystem(matrix, vector) {
    const n = vector.length;
    const augmented = matrix.map((row, i) => row.concat([vector[i]]));

    for (let col = 0; col < n; col++) {
        let pivotRow = col;
        for (let r = col + 1; r < n; r++) {
            if (Math.abs(augmented[r][col]) > Math.abs(augmented[pivotRow][col])) pivotRow = r;
        }
        [augmented[col], augmented[pivotRow]] = [augmented[pivotRow], augmented[col]];

        const pivot = augmented[col][col];
        // A near-zero pivot means the sample set doesn't actually constrain
        // this coefficient (e.g. all raw values identical) — leave it at 0
        // rather than divide by ~0, same "hold rather than guess" rule
        // droppedFrames already follows for a missing face.
        if (Math.abs(pivot) < 1e-9) continue;
        for (let j = col; j <= n; j++) augmented[col][j] /= pivot;
        for (let r = 0; r < n; r++) {
            if (r === col) continue;
            const factor = augmented[r][col];
            for (let j = col; j <= n; j++) augmented[r][j] -= factor * augmented[col][j];
        }
    }

    return augmented.map((row) => row[n]);
}

function evalPolynomial(coeffs, x) {
    let result = 0;
    let p = 1;
    for (const c of coeffs) {
        result += c * p;
        p *= x;
    }
    return result;
}

function calibratedX(x) {
    if (!calCoeffs) return x;
    const t = evalPolynomial(calCoeffs, x);
    return Math.max(0, Math.min(1, t));
}

function stepGame(g, dt) {
    if (g.over) return;

    const targetFraction = calibratedX(smoothedGazeX);
    smoothedPaddleFraction = smoothedPaddleFraction === null
        ? targetFraction
        : smoothedPaddleFraction + PADDLE_EMA_ALPHA * (targetFraction - smoothedPaddleFraction);

    g.paddleX = smoothedPaddleFraction * (g.width - PADDLE_WIDTH);
    g.paddleX = Math.max(0, Math.min(g.width - PADDLE_WIDTH, g.paddleX));

    const b = g.ball;
    b.x += b.vx * dt;
    b.y += b.vy * dt;

    if (b.x - BALL_RADIUS < 0) {
        b.x = BALL_RADIUS;
        b.vx *= -1;
    } else if (b.x + BALL_RADIUS > g.width) {
        b.x = g.width - BALL_RADIUS;
        b.vx *= -1;
    }
    if (b.y - BALL_RADIUS < 0) {
        b.y = BALL_RADIUS;
        b.vy *= -1;
    }

    const paddleY = g.height - PADDLE_Y_MARGIN - PADDLE_HEIGHT;
    if (
        b.vy > 0 &&
        b.y + BALL_RADIUS >= paddleY &&
        b.y + BALL_RADIUS <= paddleY + PADDLE_HEIGHT + Math.abs(b.vy * dt) &&
        b.x >= g.paddleX &&
        b.x <= g.paddleX + PADDLE_WIDTH
    ) {
        b.y = paddleY - BALL_RADIUS;
        b.vy *= -1;
        const hitOffset = (b.x - (g.paddleX + PADDLE_WIDTH / 2)) / (PADDLE_WIDTH / 2);
        b.vx = hitOffset * 220;
    }

    for (const brick of g.bricks) {
        if (!brick.alive) continue;
        if (
            b.x + BALL_RADIUS > brick.x &&
            b.x - BALL_RADIUS < brick.x + brick.w &&
            b.y + BALL_RADIUS > brick.y &&
            b.y - BALL_RADIUS < brick.y + brick.h
        ) {
            brick.alive = false;
            b.vy *= -1;
            g.score += 10;
            g.brickStreak += 1;
            if (g.brickStreak > 0 && g.brickStreak % 6 === 0) {
                say(SPEECH_LINES.brickStreak);
            }
            break;
        }
    }

    if (b.y - BALL_RADIUS > g.height) {
        g.lives -= 1;
        g.brickStreak = 0;
        if (g.lives <= 0) {
            g.over = true;
            g.outcome = 'lost';
            say(SPEECH_LINES.gameOver);
            setStatus('Game over.');
        } else {
            say(SPEECH_LINES.lifeLost);
            resetBall(g);
        }
    }

    if (!g.over && g.bricks.every((brick) => !brick.alive)) {
        g.over = true;
        g.outcome = 'cleared';
        say(SPEECH_LINES.levelClear);
        setStatus('Board clear.');
    }
}

function drawGame(g) {
    const { text, accent, border, bgAlt } = theme;

    ctx.clearRect(0, 0, g.width, g.height);
    ctx.fillStyle = bgAlt;
    ctx.fillRect(0, 0, g.width, g.height);

    ctx.fillStyle = accent;
    for (const brick of g.bricks) {
        if (!brick.alive) continue;
        ctx.fillRect(brick.x, brick.y, brick.w, brick.h);
    }

    ctx.fillStyle = text;
    ctx.fillRect(g.paddleX, g.height - PADDLE_Y_MARGIN - PADDLE_HEIGHT, PADDLE_WIDTH, PADDLE_HEIGHT);

    ctx.beginPath();
    ctx.arc(g.ball.x, g.ball.y, BALL_RADIUS, 0, Math.PI * 2);
    ctx.fillStyle = text;
    ctx.fill();

    ctx.strokeStyle = border;
    ctx.strokeRect(0.5, 0.5, g.width - 1, g.height - 1);

    ctx.fillStyle = text;
    ctx.font = '14px system-ui, sans-serif';
    ctx.fillText(`Score ${g.score}`, 8, 18);
    ctx.fillText(`Lives ${g.lives}`, g.width - 64, 18);

    if (g.over) {
        ctx.fillStyle = text;
        ctx.font = '20px system-ui, sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText(g.outcome === 'cleared' ? 'Board clear' : 'Game over', g.width / 2, g.height / 2);
        ctx.textAlign = 'left';
    }
}

function frame(now) {
    if (!faceLandmarker) return;

    const result = faceLandmarker.detectForVideo(els.video, now);
    updateGazeFromResult(result);

    if (mode === 'calibrating') {
        // calTarget (where to look, set by showCalStep()) and calMarker
        // (the live smoothed reading) share the same track so a visitor can
        // see how close the live signal is to the point they're looking at
        // before pressing capture.
        els.calMarker.style.left = `${smoothedGazeX * 100}%`;
    } else if (mode === 'playing' && game) {
        if (game.lastTime === null) game.lastTime = now;
        const dt = Math.min(0.05, (now - game.lastTime) / 1000);
        game.lastTime = now;

        stepGame(game, dt);
        drawGame(game);
    }

    rafId = requestAnimationFrame(frame);
}

function stopTracks() {
    if (stream) {
        for (const track of stream.getTracks()) track.stop();
        stream = null;
    }
}

function showCalStep() {
    const point = CAL_POINTS[calStepIndex];
    els.calTarget.style.left = `${point.target * 100}%`;
    els.calInstructions.textContent =
        `Point ${calStepIndex + 1} of ${CAL_POINTS.length}: look at ${point.label}, then press "Capture this point".`;
    setStatus(`Look at ${point.label} and press "Capture this point".`);
}

// CAL_EDGE_WEIGHT/CAL_INTERIOR_WEIGHT by target rather than by index, so
// the weighting stays correct if CAL_POINTS' point count or spacing ever
// changes without this function needing to change with it.
function calWeightOf(sample) {
    return sample.target === 0 || sample.target === 1 ? CAL_EDGE_WEIGHT : CAL_INTERIOR_WEIGHT;
}

function captureCalPoint() {
    const raw = recentSmoothed.length
        ? recentSmoothed.reduce((sum, v) => sum + v, 0) / recentSmoothed.length
        : smoothedGazeX;
    calSamples.push({ raw, target: CAL_POINTS[calStepIndex].target });
    calStepIndex += 1;

    if (calStepIndex < CAL_POINTS.length) {
        showCalStep();
        return;
    }

    const rawValues = calSamples.map((s) => s.raw);
    const spread = Math.max(...rawValues) - Math.min(...rawValues);
    if (spread < CAL_MIN_SEPARATION) {
        // All five points read as roughly the same value — a static/fake
        // camera hits this by construction (see F05-AC02); a real session
        // hitting it means the visitor's gaze genuinely isn't moving the
        // signal, not that the fit would be unsafe to use anyway. Same
        // "hold rather than guess" choice as an unreliable per-frame face:
        // fall back to uncalibrated rather than fit a polynomial to noise.
        calCoeffs = null;
        setStatus("Those five points read almost the same — skipping calibration for this session.");
        beginGame();
        return;
    }

    calCoeffs = fitPolynomial(calSamples, CAL_POLY_DEGREE, calWeightOf);
    beginGame();
}

function enterCalibration() {
    mode = 'calibrating';
    calStepIndex = 0;
    calSamples = [];
    els.main.hidden = true;
    els.calibrate.hidden = false;
    showCalStep();
    els.calCapture.focus();
}

function beginGame() {
    mode = 'playing';
    game = newGame();
    readTheme();
    els.calibrate.hidden = true;
    els.main.hidden = false;
    els.stop.removeAttribute('aria-disabled');
    els.stop.focus();
    setStatus('Look left and right to move the paddle.');
    say(SPEECH_LINES.start);
}

function skipCalibration() {
    calCoeffs = null;
    beginGame();
}

function recalibrate() {
    if (window.speechSynthesis) window.speechSynthesis.cancel();
    game = null;
    enterCalibration();
}

async function start() {
    if (loading) return;
    loading = true;
    els.load.setAttribute('aria-disabled', 'true');
    setStatus('Requesting camera access…');

    try {
        stream = await startCamera();
        setStatus('Loading the face model…');
        faceLandmarker = await loadFaceLandmarker();
        voice = pickVoice();
        smoothedGazeX = 0.5;
        rawGazeHistory = [];
        recentSmoothed = [];
        smoothedPaddleFraction = null;
        droppedFrames = 0;
        calCoeffs = null;

        els.gate.hidden = true;
        enterCalibration();

        rafId = requestAnimationFrame(frame);
    } catch (err) {
        stopTracks();
        faceLandmarker = null;
        setStatus(`Could not start: ${err.message || err}`);
    } finally {
        loading = false;
        els.load.removeAttribute('aria-disabled');
    }
}

function stop() {
    if (rafId !== null) {
        cancelAnimationFrame(rafId);
        rafId = null;
    }
    if (faceLandmarker) {
        faceLandmarker.close();
        faceLandmarker = null;
    }
    stopTracks();
    if (window.speechSynthesis) window.speechSynthesis.cancel();
    els.video.srcObject = null;
    game = null;
    mode = 'idle';
    calCoeffs = null;

    els.main.hidden = true;
    els.calibrate.hidden = true;
    els.gate.hidden = false;
    setStatus('');
    els.load.focus();
}

if (window.speechSynthesis) {
    window.speechSynthesis.addEventListener('voiceschanged', () => {
        voice = pickVoice();
    });
}

if (window.matchMedia) {
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
        if (game) readTheme();
    });
}

els.load.removeAttribute('aria-disabled');
els.stop.setAttribute('aria-disabled', 'true');
els.gate.hidden = false;

els.load.addEventListener('click', start);
els.stop.addEventListener('click', stop);
els.calCapture.addEventListener('click', captureCalPoint);
els.calSkip.addEventListener('click', skipCalibration);
els.recalibrate.addEventListener('click', recalibrate);
