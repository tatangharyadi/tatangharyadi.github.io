// Iris: a Breakout-style game whose paddle follows the visitor's gaze.
//
// The boundary shape below — start camera, load FaceLandmarker, run
// detectForVideo() every frame, stop and release everything on demand — is
// the same one js/twin.js uses, reusing Twin's already-vendored runtime and
// already-committed model (see specs/F05_IRIS.md, F05-AC03). What differs
// is what happens with the landmarks: Twin deforms a mesh with all 468 face
// points; Iris reads only the ten iris points (indices 468-477, Twin's own
// code discards these) to drive a paddle, and never touches the DOM with a
// 3D renderer at all — the "renderer" here is a plain 2D canvas.

import { FaceLandmarker, FilesetResolver } from '../vendor/mediapipe/tasks-vision/vision_bundle.mjs';

const WASM_BASE = 'vendor/mediapipe/tasks-vision/wasm';
const FACE_MODEL_URL = 'assets/models/face-landmarker/face_landmarker.task';

// FaceLandmarker's 478-point layout gives each eye an iris centre plus its
// own pair of horizontal corner landmarks. The centre's raw x moves just as
// much when the head translates as when the eye itself moves in its
// socket — averaging the raw landmark.x values (an earlier version of this
// file did exactly that) tracks head position, not gaze, which is exactly
// what real-camera testing surfaced. Normalizing each iris centre against
// its own eye's corner-to-corner span cancels head translation because
// both corners move with the head by the same amount the iris does.
const RIGHT_IRIS_CENTER = 468;
const RIGHT_EYE_OUTER = 33;
const RIGHT_EYE_INNER = 133;
const LEFT_IRIS_CENTER = 473;
const LEFT_EYE_INNER = 362;
const LEFT_EYE_OUTER = 263;

// Dividing by an eye's own corner span (below) amplifies whatever
// landmark-detection noise was already there, in rough proportion to how
// small that span is relative to the frame — real testing showed this as
// the paddle visibly twitching with no eye movement at all. Two guards
// against that, applied before the EMA below: a per-eye reading is
// discarded outright if its span is too small to trust, and the combined
// per-frame reading is median-filtered over a short window so a single
// noisy detection can't move the paddle — only a run of several
// consistent frames can. Neither constant is verified against real
// hardware yet; F05-AC02 is exactly the test that tells us whether either
// needs to move.
const MIN_EYE_SPAN = 0.02;
const RAW_GAZE_MEDIAN_WINDOW = 5;
const IRIS_X_EMA_ALPHA = 0.15;

// Looking left is looking toward the camera's right in an unmirrored feed
// (the camera faces the visitor). Flip so "look left" moves the paddle
// left from the visitor's own point of view. Flip this if AC02 testing
// shows it feels backwards.
const MIRROR_GAZE_X = true;

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
        outputFaceBlendshapes: false,
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

// Where the iris centre sits between the eye's own two corners, as a 0..1
// fraction, or null if the span is too small to trust (near-profile head
// angle, partial occlusion, a bad detection). Order-independent (min/max
// rather than assuming which corner has the smaller x) because the two
// eyes' corner pairs run in opposite left-right order in FaceLandmarker's
// output. A small span is exactly where dividing by it turns ordinary
// landmark jitter into a large swing in the result, so it is discarded
// rather than trusted.
function eyeRatio(iris, cornerA, cornerB) {
    const lo = Math.min(cornerA.x, cornerB.x);
    const hi = Math.max(cornerA.x, cornerB.x);
    if (hi - lo < MIN_EYE_SPAN) return null;
    return (iris.x - lo) / (hi - lo);
}

function medianOf(values) {
    const sorted = values.slice().sort((a, b) => a - b);
    return sorted[Math.floor(sorted.length / 2)];
}

// Recent raw (pre-EMA) readings, oldest first, capped at
// RAW_GAZE_MEDIAN_WINDOW — see the constant's comment for why this exists.
let rawGazeHistory = [];

function updateGazeFromLandmarks(landmarks) {
    const rightIris = landmarks[RIGHT_IRIS_CENTER];
    const rightOuter = landmarks[RIGHT_EYE_OUTER];
    const rightInner = landmarks[RIGHT_EYE_INNER];
    const leftIris = landmarks[LEFT_IRIS_CENTER];
    const leftInner = landmarks[LEFT_EYE_INNER];
    const leftOuter = landmarks[LEFT_EYE_OUTER];
    if (!rightIris || !rightOuter || !rightInner || !leftIris || !leftInner || !leftOuter) return;

    const rightSpan = Math.abs(rightOuter.x - rightInner.x);
    const leftSpan = Math.abs(leftInner.x - leftOuter.x);
    const rightRatio = eyeRatio(rightIris, rightOuter, rightInner);
    const leftRatio = eyeRatio(leftIris, leftInner, leftOuter);
    const ratios = [rightRatio, leftRatio].filter((r) => r !== null);
    if (ratios.length === 0) {
        droppedFrames += 1;
        if (DEBUG) renderDebug({ rightRatio, leftRatio, rightSpan, leftSpan, rawX: null, medianX: null });
        return; // both eyes unreliable this frame; hold the last position rather than guess
    }

    const rawX = ratios.reduce((sum, r) => sum + r, 0) / ratios.length;
    rawGazeHistory.push(rawX);
    if (rawGazeHistory.length > RAW_GAZE_MEDIAN_WINDOW) rawGazeHistory.shift();
    const medianX = medianOf(rawGazeHistory);

    const gazeX = MIRROR_GAZE_X ? 1 - medianX : medianX;
    smoothedGazeX = smoothedGazeX + IRIS_X_EMA_ALPHA * (gazeX - smoothedGazeX);

    if (DEBUG) renderDebug({ rightRatio, leftRatio, rightSpan, leftSpan, rawX, medianX });
}

function fmt(n) {
    return n === null || n === undefined ? ' -- ' : n.toFixed(3);
}

function renderDebug(f) {
    els.debug.textContent =
        `right ratio ${fmt(f.rightRatio)}  span ${fmt(f.rightSpan)}\n` +
        `left  ratio ${fmt(f.leftRatio)}  span ${fmt(f.leftSpan)}\n` +
        `raw ${fmt(f.rawX)}  median ${fmt(f.medianX)}  smoothed ${fmt(smoothedGazeX)}\n` +
        `dropped frames (both eyes unreliable): ${droppedFrames}`;
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
    const landmarks = result.faceLandmarks && result.faceLandmarks[0];
    if (landmarks) updateGazeFromLandmarks(landmarks);

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
