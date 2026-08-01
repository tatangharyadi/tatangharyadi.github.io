'use strict';

const nextButton = document.getElementById('next');
const prevButton = document.getElementById('prev');
const carousel = document.querySelector('.casestudy--container');
const listHTML = document.querySelector('.casestudy--list');

const NEXT = 'next';
const PREV = 'prev';

const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)');

/* Single source of truth for the slide duration lives in css/style.css as
   --casestudy-slide-duration, so changing the animation there cannot drift out
   of sync with how long the buttons stay locked here. */
const slideDuration = () => {
    const raw = getComputedStyle(document.documentElement)
        .getPropertyValue('--casestudy-slide-duration')
        .trim();
    const ms = raw.endsWith('ms') ? parseFloat(raw) : parseFloat(raw) * 1000;
    return Number.isFinite(ms) ? ms : 700;
};

let isSliding = false;
let settleTimer;

/* aria-disabled rather than the disabled property: disabling the element the
   user just activated moves focus to <body>, which strands keyboard users
   outside the carousel. Re-entry is already blocked by the isSliding guard, so
   this only needs to communicate the state, not enforce it. */
const setButtonsEnabled = (enabled) => {
    nextButton.setAttribute('aria-disabled', String(!enabled));
    prevButton.setAttribute('aria-disabled', String(!enabled));
};

const settle = () => {
    clearTimeout(settleTimer);
    carousel.classList.remove(NEXT, PREV);
    setButtonsEnabled(true);
    isSliding = false;
};

const showSlider = (type) => {
    if (isSliding) return;

    const items = listHTML.querySelectorAll('.casestudy--item');
    if (items.length < 2) return;

    if (type === NEXT) {
        listHTML.appendChild(items[0]);
    } else {
        listHTML.prepend(items[items.length - 1]);
    }

    /* Reduced motion: the CSS animations are disabled, so animating would only
       leave the buttons locked waiting for an event that never fires. */
    if (reduceMotion.matches) {
        settle();
        return;
    }

    isSliding = true;
    setButtonsEnabled(false);

    carousel.classList.remove(NEXT, PREV);
    void carousel.offsetWidth; // reflow, so re-adding the class restarts the animations
    carousel.classList.add(type);

    settleTimer = setTimeout(settle, slideDuration());
};

nextButton.addEventListener('click', () => showSlider(NEXT));
prevButton.addEventListener('click', () => showSlider(PREV));

carousel.addEventListener('keydown', (event) => {
    if (event.key === 'ArrowRight') {
        event.preventDefault();
        showSlider(NEXT);
    } else if (event.key === 'ArrowLeft') {
        event.preventDefault();
        showSlider(PREV);
    }
});
