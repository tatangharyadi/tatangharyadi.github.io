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

const IRIS_LANDMARK_START = 468;
const IRIS_LANDMARK_END = 478; // exclusive; 468-477 inclusive, 10 points

// Frame-to-frame iris position is noisy; this is the first smoothing
// constant, not a verified one — F05-AC02 is exactly the test that tells us
// whether it needs to move.
const IRIS_X_EMA_ALPHA = 0.25;

// Looking left is looking toward the camera's right in an unmirrored feed
// (the camera faces the visitor). Flip so "look left" moves the paddle
// left from the visitor's own point of view. Flip this if AC02 testing
// shows it feels backwards.
const MIRROR_GAZE_X = true;

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
    main: document.getElementById('iris'),
    video: document.getElementById('iris--video'),
    stage: document.getElementById('iris--stage'),
    stop: document.getElementById('iris--stop'),
};

const ctx = els.stage.getContext('2d');

let loading = false;
let stream = null;
let faceLandmarker = null;
let rafId = null;
let smoothedGazeX = 0.5;
let voice = null;

let game = null; // set up fresh each start()

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

function updateGazeFromLandmarks(landmarks) {
    let sum = 0;
    let n = 0;
    for (let i = IRIS_LANDMARK_START; i < IRIS_LANDMARK_END; i++) {
        const p = landmarks[i];
        if (!p) continue;
        sum += p.x;
        n++;
    }
    if (n === 0) return;
    const rawX = sum / n;
    const gazeX = MIRROR_GAZE_X ? 1 - rawX : rawX;
    smoothedGazeX = smoothedGazeX + IRIS_X_EMA_ALPHA * (gazeX - smoothedGazeX);
}

function stepGame(g, dt) {
    if (g.over) return;

    g.paddleX = smoothedGazeX * (g.width - PADDLE_WIDTH);
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

    if (game.lastTime === null) game.lastTime = now;
    const dt = Math.min(0.05, (now - game.lastTime) / 1000);
    game.lastTime = now;

    stepGame(game, dt);
    drawGame(game);

    rafId = requestAnimationFrame(frame);
}

function stopTracks() {
    if (stream) {
        for (const track of stream.getTracks()) track.stop();
        stream = null;
    }
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

        game = newGame();
        smoothedGazeX = 0.5;
        readTheme();

        els.gate.hidden = true;
        els.main.hidden = false;
        els.stop.removeAttribute('aria-disabled');
        els.stop.focus();
        setStatus('Look left and right to move the paddle.');
        say(SPEECH_LINES.start);

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

    els.main.hidden = true;
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
