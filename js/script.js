const texts = {
    "text1": "I have launched a portfolio of products for retail, manufacturing, and F&B industries. I successfully scaled up social commerce platforms in Indonesia, Vietnam and the Philippines with 1.5 million members. I have turned around multi-million dollar digital transformation projects.",
    "text2": "Early in my career, I built offshore IT services startups. Later, I implemented ERP systems, IT operations consolidation and drove omnichannel initiatives at public listed and multinational companies. Then, I developed a SaaS product for a food service startup.",
    "text3": "My technology stack includes Flutter (Firebase, Realm), Typescript (React, Next.js), Go (microservices), Github Actions, GCP (CloudRun, Pub/Sub, PostgreSQL).",
};

function handleOnClick(key) {
    document.getElementById('hero-text').innerText = texts[key];
}