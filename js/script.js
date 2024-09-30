const texts = {
    "text1": "I have launched a portfolio of products for retail, manufacturing, and F&B industries. I successfully scaled up social commerce platforms in Indonesia, Vietnam and the Philippines with 1.5 million members. I have turned around multi-million dollar digital transformation projects.",
    "text2": "Early in my career, I built offshore IT services startups. Later, I implemented ERP systems, IT operations consolidation and drove omnichannel initiatives at public listed and multinational companies. Then, I developed a SaaS product for a food service startup.",
    "text3": "My technology stack includes Flutter (Firebase, Realm), Typescript (React, Next.js), Go (microservices), Github Actions, GCP (CloudRun, Pub/Sub, PostgreSQL).",
};

function handleOnClick(key) {
    document.getElementById('hero-text').innerText = texts[key];
}

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
