let nextButton = document.getElementById('next');
let prevButton = document.getElementById('prev');
let carousel = document.querySelector('.casestudy--container');
let listHTML = document.querySelector('.casestudy--list');

nextButton.onclick = function() {
    showSlider(next);
}
prevButton.onclick = function() {
    showSlider(prev);
}

let unAcceptClick;
const showSlider = (type) => {
    nextButton.style.pointerEvents = 'none';
    prevButton.style.pointerEvents = 'none';

    let items = document.querySelectorAll('.casestudy--item');
    if(type === next) {
        listHTML.appendChild(items[0]);
    } else {
        let positionLast = items.length - 1;
        listHTML.prepend(items[positionLast]);
    }

    clearTimeout(unAcceptClick);
    unAcceptClick = setTimeout(() => {
        nextButton.style.pointerEvents = 'auto';
        prevButton.style.pointerEvents = 'auto';
    }, 1000);
}
