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

// Below this separation between the two captured points, calibration
// treats the pair as noise rather than a real range — smoothedGazeX is
// already EMA-smoothed, so two captures this close together are almost
// certainly the same point measured twice, not a deliberate look-left vs.
// look-right pair.
const CAL_MIN_SEPARATION = 0.03;

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
    calMarker: document.getElementById('iris--cal-marker'),
    calLeft: document.getElementById('iris--cal-left'),
    calRight: document.getElementById('iris--cal-right'),
    calStart: document.getElementById('iris--cal-start'),
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

// Raw smoothedGazeX captured at each calibration press; calMin/calMax are
// the range stepGame() actually maps through. Null means uncalibrated
// (skipped), which is the pre-calibration behaviour: smoothedGazeX used
// directly as the 0..1 fraction of the paddle track.
let calLeftRaw = null;
let calRightRaw = null;
let calMin = null;
let calMax = null;

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
        outputFacialTransformationMatrixes: false,
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

function updateGazeFromResult(result) {
    const blendshapes = result.faceBlendshapes && result.faceBlendshapes[0];
    if (!blendshapes) {
        droppedFrames += 1;
        if (DEBUG) renderDebug({ right: null, left: null, rawX: null, medianX: null });
        return; // no face this frame; hold the last position rather than guess
    }

    const { right, left, score } = gazeScoreFromBlendshapes(blendshapes.categories);
    // right/left are each 0..1 confidences, not a position — center on 0.5
    // and let a stronger "look right" than "look left" (or vice versa) push
    // away from it in either direction.
    const rawX = Math.min(1, Math.max(0, 0.5 + (right - left) / 2));
    rawGazeHistory.push(rawX);
    if (rawGazeHistory.length > RAW_GAZE_MEDIAN_WINDOW) rawGazeHistory.shift();
    const medianX = medianOf(rawGazeHistory);

    const gazeX = MIRROR_GAZE_X ? 1 - medianX : medianX;
    smoothedGazeX = smoothedGazeX + IRIS_X_EMA_ALPHA * (gazeX - smoothedGazeX);

    if (DEBUG) renderDebug({ right, left, rawX, medianX, score });
}

function fmt(n) {
    return n === null || n === undefined ? ' -- ' : n.toFixed(3);
}

function renderDebug(f) {
    // calMax - calMin is the thing to watch: calibratedX divides by it, so a
    // narrow-but-passing capture (anything just above CAL_MIN_SEPARATION)
    // turns ordinary smoothedGazeX wander into a much larger paddle swing.
    const range = calMin === null ? null : calMax - calMin;
    const gain = range === null ? 1 : 1 / range;
    const s = f.score || {};
    els.debug.textContent =
        `eyeLookOutRight ${fmt(s.eyeLookOutRight)}  eyeLookInLeft  ${fmt(s.eyeLookInLeft)}  -> right ${fmt(f.right)}\n` +
        `eyeLookInRight  ${fmt(s.eyeLookInRight)}  eyeLookOutLeft ${fmt(s.eyeLookOutLeft)}  -> left  ${fmt(f.left)}\n` +
        `raw ${fmt(f.rawX)}  median ${fmt(f.medianX)}  smoothed ${fmt(smoothedGazeX)}\n` +
        `calMin ${fmt(calMin)}  calMax ${fmt(calMax)}  range ${fmt(range)}  gain ${gain.toFixed(1)}x\n` +
        `paddle fraction ${fmt(calibratedX(smoothedGazeX))}\n` +
        `dropped frames (no face this frame): ${droppedFrames}`;
}

function calibratedX(x) {
    if (calMin === null || calMax === null) return x;
    const t = (x - calMin) / (calMax - calMin);
    return Math.max(0, Math.min(1, t));
}

function stepGame(g, dt) {
    if (g.over) return;

    g.paddleX = calibratedX(smoothedGazeX) * (g.width - PADDLE_WIDTH);
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

function updateCalStartEnabled() {
    const ready =
        calLeftRaw !== null &&
        calRightRaw !== null &&
        Math.abs(calRightRaw - calLeftRaw) >= CAL_MIN_SEPARATION;
    if (ready) {
        els.calStart.removeAttribute('aria-disabled');
    } else {
        els.calStart.setAttribute('aria-disabled', 'true');
    }
    return ready;
}

function captureLeft() {
    calLeftRaw = smoothedGazeX;
    updateCalStartEnabled();
    setStatus('Left captured. Now look at the right edge and press "Capture right".');
}

function captureRight() {
    calRightRaw = smoothedGazeX;
    updateCalStartEnabled();
    setStatus('Right captured. Press "Start playing" when ready.');
}

function enterCalibration() {
    mode = 'calibrating';
    calLeftRaw = null;
    calRightRaw = null;
    updateCalStartEnabled();
    els.main.hidden = true;
    els.calibrate.hidden = false;
    setStatus('Look at the left edge of your screen and press "Capture left".');
    els.calLeft.focus();
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

function finishCalibration() {
    if (!updateCalStartEnabled()) return;
    calMin = Math.min(calLeftRaw, calRightRaw);
    calMax = Math.max(calLeftRaw, calRightRaw);
    beginGame();
}

function skipCalibration() {
    calMin = null;
    calMax = null;
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
        droppedFrames = 0;
        calMin = null;
        calMax = null;

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
    calMin = null;
    calMax = null;

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
els.calLeft.addEventListener('click', captureLeft);
els.calRight.addEventListener('click', captureRight);
els.calStart.addEventListener('click', finishCalibration);
els.calSkip.addEventListener('click', skipCalibration);
els.recalibrate.addEventListener('click', recalibrate);
