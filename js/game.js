// Helm: the browser half.
//
// The simulation is assets/game.wasm, built from game/ by scripts/build_game.sh.
// This file owns the DOM and owns nothing else. It does not know what a good is
// worth, how far a ship can see, or which way the wind blows in March; it asks.
// Every rule lives in the Rust, and the two talk over the narrowest boundary
// that would carry the traffic.
//
// There is no wasm-bindgen here, so the boundary is worth stating plainly.
// Exports are plain `extern "C"` functions, which means the only things that
// cross are numbers. Anything larger crosses as a region of the module's linear
// memory that we read from this side:
//
//   render_ptr()/render_len()  two bytes per hex, terrain code then fog state,
//                              in row-major offset order. Valid until the next
//                              call that moves the world.
//   text_ptr()/text_len()      a UTF-8 JSON document written by whichever of
//                              write_atlas / write_status / write_look was
//                              called last. It reuses one buffer, so it is
//                              valid only until the next of those calls.
//
// THE RULE THAT MATTERS: never hold on to a typed array over wasm memory.
// Rust's allocator will grow the module's memory as the game runs, and
// `memory.grow` detaches every existing ArrayBuffer view in JavaScript. A view
// cached at load time works for a few moves and then silently reads zeroes,
// which presents as a blank chart with nothing in the console. So both helpers
// below re-derive the pointer *and* the view on every single call, and nothing
// else in this file is allowed to touch `memory.buffer`.

const WASM_URL = "assets/game.wasm";

// How much of the world is on screen. The world is 72 by 36; showing all of it
// would be five thousand SVG nodes restyled on every keystroke, to draw a chart
// that is mostly fog. The window travels with the ship instead.
const VIEW_COLS = 25;
const VIEW_ROWS = 17;

// Pointy-top hex geometry, from redblobgames.com/grids/hexagons. Width is
// sqrt(3) * size and the rows overlap, so vertical spacing is 3/2 * size.
const SIZE = 13;
const HEX_W = Math.sqrt(3) * SIZE;
const HEX_H = 1.5 * SIZE;

// Terrain codes, matching the constants in game/src/sim.rs.
// Index 6 is a merchant, and it reads as a rounder, softer mark than the
// raider's X on purpose: the two are the only things on the chart you can meet,
// and the one you are allowed to shoot at should not look like the one you are
// not.
// Index 7 is a king's ship: a triangle, because a man-of-war under a press of
// sail is the one thing out here that is bigger than you.
// Indices 8, 9 and 10 are the same three with more than one on the hex. A hex
// draws one glyph, so without these a crowded hex and a quiet one look
// identical and the count in the status column cannot be reconciled with the
// chart. Each doubles its own mark rather than introducing a new shape: the
// reader has already learnt o and X and ▲, and a stack is a quantity of the
// thing they know, not a different thing.
const GLYPH = ["~", "≈", "#", "⌂", "@", "X", "o", "▲", "XX", "oo", "▲▲"];

// q e / a d / z c, laid out the way the six hex directions actually point.
// The indices are hex::DIRECTIONS order: 0 E, 1 NE, 2 NW, 3 W, 4 SW, 5 SE.
const KEYS = { q: 2, e: 1, a: 3, d: 0, z: 4, c: 5 };

const SVG_NS = "http://www.w3.org/2000/svg";

let wasm = null;
let atlas = null;
let cols = 0;
let rows = 0;
let cells = []; // one entry per viewport hex, created once and then reused
let looking = null; // {col, row} the "there" panel is describing

// -- the two ways across the boundary --------------------------------------

/** The render buffer, as a fresh view. Never stored. See the note above. */
function renderBytes() {
  const { render_ptr, render_len, memory } = wasm;
  return new Uint8Array(memory.buffer, render_ptr(), render_len());
}

/**
 * Whatever the last write_* call put in the text buffer, parsed.
 *
 * The decode has to happen before anything else calls into wasm, because the
 * buffer is reused. Passing a subarray rather than the whole heap means
 * TextDecoder copies out exactly the bytes we want and we are done with the
 * view immediately.
 */
const decoder = new TextDecoder("utf-8");
function takeText() {
  const { text_ptr, text_len, memory } = wasm;
  const bytes = new Uint8Array(memory.buffer, text_ptr(), text_len());
  return JSON.parse(decoder.decode(bytes));
}

// -- small DOM helpers ------------------------------------------------------

const $ = (id) => document.getElementById(id);

function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

const MONTHS = ["", "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December"];

const TIERS = ["none", "I", "II", "III", "IV"];
const STRENGTH = ["calm", "light", "fresh", "strong"];
const WEATHER = ["clear", "unsettled", "dirty", "a storm"];

/** Coordinates as a navigator would say them rather than as an array index. */
function latlon(col, row) {
  const lat = 87.5 - 5 * row;
  const lon = 5 * col - 180;
  const ns = lat >= 0 ? "N" : "S";
  const ew = lon >= 0 ? "E" : "W";
  return `${Math.abs(lat).toFixed(0)}°${ns} ${Math.abs(lon).toFixed(0)}°${ew}`;
}

// -- the chart --------------------------------------------------------------

/**
 * Build the viewport once.
 *
 * Only the glyph and its class change afterwards, which keeps a move to a few
 * hundred attribute writes rather than a few thousand node creations. The
 * geometry can be static because `origin()` below always picks an even top row,
 * so a viewport row and the world row under it always share parity, and in
 * odd-r offset coordinates parity is the only thing that shifts a row sideways.
 */
function buildChart() {
  const svg = $("chart");
  const width = HEX_W * (VIEW_COLS + 1);
  const height = HEX_H * (VIEW_ROWS + 1) + SIZE;
  svg.setAttribute("viewBox", `0 0 ${width.toFixed(1)} ${height.toFixed(1)}`);

  for (let vr = 0; vr < VIEW_ROWS; vr++) {
    for (let vc = 0; vc < VIEW_COLS; vc++) {
      const x = HEX_W * (vc + 0.5 * (vr & 1) + 0.5);
      const y = HEX_H * vr + SIZE;

      const glyph = document.createElementNS(SVG_NS, "text");
      glyph.setAttribute("x", x.toFixed(1));
      glyph.setAttribute("y", y.toFixed(1));
      glyph.setAttribute("class", "cell seen-0");
      svg.appendChild(glyph);

      // A transparent circle over each hex, so a click has something to land on
      // even where the glyph is a single narrow character or nothing at all.
      const hit = document.createElementNS(SVG_NS, "circle");
      hit.setAttribute("cx", x.toFixed(1));
      hit.setAttribute("cy", y.toFixed(1));
      hit.setAttribute("r", (SIZE * 0.9).toFixed(1));
      hit.setAttribute("class", "cell--hit");
      svg.appendChild(hit);

      cells.push({ glyph, hit, col: 0, row: 0 });
    }
  }

  const mark = document.createElementNS(SVG_NS, "circle");
  mark.setAttribute("r", (SIZE * 0.85).toFixed(1));
  mark.setAttribute("class", "cell--mark");
  mark.setAttribute("cx", "-99");
  mark.setAttribute("cy", "-99");
  mark.id = "chart--mark";
  svg.appendChild(mark);

  svg.addEventListener("click", (event) => {
    const index = cells.findIndex((c) => c.hit === event.target);
    if (index >= 0) lookAt(cells[index].col, cells[index].row);
  });
}

/**
 * Top-left world cell of the viewport.
 *
 * The column wraps, because the world does: sailing west out of the Atlantic
 * comes round to Japan. The row is clamped instead, because the map has a top
 * and a bottom and this game does not model crossing the pole. The row is also
 * forced even, which is what lets the geometry above stay static.
 */
function origin(shipCol, shipRow) {
  const col = ((shipCol - (VIEW_COLS >> 1)) % cols + cols) % cols;
  let row = shipRow - (VIEW_ROWS >> 1);
  row = Math.max(0, Math.min(rows - VIEW_ROWS, row));
  return { col, row: row & ~1 };
}

function drawChart(shipCol, shipRow) {
  const bytes = renderBytes();
  const o = origin(shipCol, shipRow);
  let markX = -99;
  let markY = -99;

  for (let vr = 0; vr < VIEW_ROWS; vr++) {
    for (let vc = 0; vc < VIEW_COLS; vc++) {
      const cell = cells[vr * VIEW_COLS + vc];
      const col = (o.col + vc) % cols;
      const row = o.row + vr;
      cell.col = col;
      cell.row = row;

      if (row < 0 || row >= rows) {
        cell.glyph.setAttribute("class", "cell seen-0");
        cell.glyph.textContent = "";
        continue;
      }

      const at = (row * cols + col) * 2;
      const code = bytes[at];
      const seen = bytes[at + 1];
      cell.glyph.textContent = seen === 0 ? "" : GLYPH[code];
      cell.glyph.setAttribute("class", `cell seen-${seen} code-${code}`);

      if (looking && looking.col === col && looking.row === row) {
        markX = HEX_W * (vc + 0.5 * (vr & 1) + 0.5);
        markY = HEX_H * vr + SIZE;
      }
    }
  }

  const mark = $("chart--mark");
  mark.setAttribute("cx", markX.toFixed(1));
  mark.setAttribute("cy", markY.toFixed(1));
}

// -- the status column ------------------------------------------------------

function fact(dl, label, value, className) {
  dl.appendChild(el("dt", null, label));
  dl.appendChild(el("dd", className, value));
}

function drawWho(s) {
  const dl = $("who--facts");
  dl.replaceChildren();
  fact(dl, "gold", s.gold.toLocaleString("en"));
  fact(dl, "date", `${s.day} ${MONTHS[s.month]} ${s.year}`);
  fact(dl, "position", latlon(s.col, s.row));
  fact(dl, "ship", `${s.shipName} · ${s.shipRig}`);
  // Stores are counted into the hold because they are stowed in it. Printing
  // the cargo figure alone would say 5/40 to a player with no room left, which
  // is the one number on this panel a trader plans against.
  fact(dl, "hull", `${TIERS[s.hull]} · ${s.cargo + s.stores}/${s.capacity} in hold`);
  fact(dl, "rigging",
    `${TIERS[s.rigging]} · ${s.shipBluewater ? "blue water" : `${s.bluewater} hex offing`}`);
  // A broadside there is nobody to serve is the only figure here that would
  // otherwise lie, so short-handed guns are printed as both numbers.
  fact(dl, "guns",
    s.guns === 0 ? "none"
      : s.gunsWorked === s.guns ? `${s.guns}`
      : `${s.gunsWorked} of ${s.guns} worked`,
    s.guns > 0 && s.gunsWorked < s.guns ? "bad" : null);
  fact(dl, "crew", `${s.crew} of ${s.crewMin}–${s.crewMax} · ${s.wages} a month`,
    s.crew < s.crewMin ? "bad" : null);
  // Days rather than units, because units are only meaningful against a crew
  // size the player is about to change. Three days is the same threshold the
  // Rust warns at, so the panel and the chronicle agree.
  fact(dl, "stores",
    s.crew === 0 ? "nobody left to feed"
      : `${s.food} food, ${s.water} water · ${s.daysOfStores} days`,
    s.daysOfStores <= 3 ? "bad" : null);
  fact(dl, "lumber",
    s.lumber === 0 ? "none" : `${s.lumber} · mends ${s.mendable}%`);
  fact(dl, "damage", `${s.damage}%`, s.damage > 50 ? "bad" : null);
  fact(dl, "offing", `${s.offshore} from land`,
    s.offshore > s.bluewater ? "bad" : null);
  fact(dl, "wind", `${STRENGTH[s.windStrength]} from ${atlas.directions[(s.windDir + 3) % 6]}`);
  fact(dl, "current", s.currentStrength === 0 ? "slack" : `setting ${atlas.directions[s.currentDir]}`);
  fact(dl, "weather", WEATHER[s.weather], s.weather >= 2 ? "bad" : null);
  fact(dl, "in sight", inSight(s), s.pirates + s.navy > 0 ? "bad" : null);
  // Both halves of the standing, because neither is enough on its own. The
  // phrase is what the mechanic actually means and the number is the only way
  // to see it moving, since a band is thirty-five points wide and a fight is
  // worth two.
  fact(dl, "reputation", `${s.standing} (${s.reputation > 0 ? "+" : ""}${s.reputation})`,
    s.reputation <= -15 ? "bad" : null);
  // Only shown while it is true, and that is the point: a raider that has lost
  // sight of you keeps this up for a day and a half, so a warning that stays
  // lit after you think you are clear is the memory being legible.
  // Shown whether or not any of them is over the horizon yet, because the
  // threat is the hunt and not the sighting, and a player who cannot see the
  // fleet growing has no way to read what raiding actually cost them.
  if (s.navyOut > 0) {
    fact(dl, "wanted",
      s.navyOut === 1 ? "a king's ship is looking for you"
        : `${s.navyOut} king's ships are looking for you`, "bad");
  }
  if (s.hunted) fact(dl, "warning", "you are being hunted", "bad");
}

/** What the lookout has, counting raiders, traders and the crown separately. */
function inSight(s) {
  const parts = [];
  if (s.pirates > 0) parts.push(`${s.pirates} strange sail`);
  if (s.navy > 0) parts.push(s.navy === 1 ? "a king's ship" : `${s.navy} king's ships`);
  if (s.merchants > 0) parts.push(s.merchants === 1 ? "a trader" : `${s.merchants} traders`);
  return parts.length === 0 ? "nothing" : parts.join(", ");
}

/**
 * The units-per-order box. One control, shared by the market and the chandler,
 * because "how many" is one question and two boxes would be two answers to it.
 *
 * It is a real `<input type="number">` on purpose: the keyboard handler lets a
 * reader type by checking the tag name, so anything else here would swallow
 * the steering keys.
 */
function qtyControl() {
  const qty = el("p");
  // "how many" rather than "units per order", because the same box now sets
  // the size of a purchase and the number of hands signed on, and hands are
  // not units.
  const label = el("label", null, "how many ");
  label.htmlFor = "qty";
  const input = document.createElement("input");
  input.type = "number";
  input.id = "qty";
  input.min = "1";
  input.value = String(currentQty);
  input.addEventListener("change", () => {
    currentQty = Math.max(1, parseInt(input.value, 10) || 1);
  });
  qty.appendChild(label);
  qty.appendChild(input);
  return qty;
}

/**
 * Food, water and lumber, on the quay or out of another ship's hold.
 *
 * The same three orders in both places, at two prices, and the price shown is
 * the one the order will actually charge: the Rust decides that from where the
 * ship is, and the page reads the matching row rather than working it out.
 * Selling back is quayside only, so the button simply is not there at sea.
 */
function chandler(s, body, afloat) {
  const prices = afloat ? s.storePricesAfloat : s.storePrices;
  const row = el("div", "helm--orders");
  s.storeNames.forEach((name, i) => {
    const b = el("button", null, `${name}, ${prices[i]} each`);
    b.id = `store-buy-${name}`;
    b.addEventListener("click", () => order(() => wasm.provision(i, currentQty)));
    row.appendChild(b);
  });
  body.appendChild(row);
  if (afloat) return;
  const back = el("div", "helm--orders");
  s.storeNames.forEach((name, i) => {
    const aboard = s[name];
    const b = el("button", null, `sell ${name}`);
    b.id = `store-sell-${name}`;
    // Half of what it cost, and nothing at all if there is none aboard. The
    // press still goes through so the chronicle can say which of those it was.
    if (aboard === 0) b.setAttribute("aria-disabled", "true");
    b.addEventListener("click", () => order(() => wasm.provision(i, -currentQty)));
    back.appendChild(b);
  });
  body.appendChild(back);
}

/**
 * Timber and hands against damage, which is the only repair available at sea.
 *
 * Both halves have to be there and the button says which one is missing,
 * because "mend" doing nothing is indistinguishable from a broken page.
 */
function mendButton(s) {
  const short = s.crew < s.crewMin;
  const b = el("button", null,
    short ? `too few hands to mend, ${s.crewMin} needed`
      : s.damage === 0 ? "sound, nothing to mend"
      : s.mendable === 0 ? "no lumber aboard"
      // Named for who does the work and priced in what it actually costs,
      // because the button beside it in a shipyard is the same repair bought
      // in a different currency. "20 lumber and a day" against "800 gold and
      // two days" is a choice; "mend" against "repair" is a guess.
      : `carpenter: ${s.mendable}% for ${s.mendable} lumber and a day`);
  b.id = "order-mend";
  if (short || s.mendable === 0) b.setAttribute("aria-disabled", "true");
  b.addEventListener("click", () => order(() => wasm.mend()));
  return b;
}

/** Buy and sell rows, plus the yard, or an explanation of why there is neither. */
function drawHere(s) {
  const body = $("here--body");
  body.replaceChildren();

  if (s.lost) {
    body.appendChild(el("p", "bad", "The ship is lost. Start a new voyage."));
    return;
  }

  if (s.port < 0) {
    body.appendChild(el("p", null, "At sea. Nothing to trade with but the weather."));
    if (s.merchantHere) {
      body.appendChild(el("p", null,
        "A trader lies alongside, hailing you. Running out the guns would cost you your name."));
      // She will sell out of her own hold at half as much again. This is the
      // only way to victual away from a harbour, and it is the difference
      // between a bad passage and a dead crew.
      body.appendChild(el("h3", "panel--sub", "her stores, at her price"));
      chandler(s, body, true);
      body.appendChild(qtyControl());
    }
    // A battle happens at sea, so the repair the lumber is for has to be
    // reachable at sea. The shipwright is a harbour away and the crew are not.
    body.appendChild(el("h3", "panel--sub", "the carpenter"));
    const carpenter = el("div", "helm--orders");
    carpenter.appendChild(mendButton(s));
    body.appendChild(carpenter);
    if (s.underWay) {
      body.appendChild(el("p", "dim", "A course is laid. Sail on to make the next leg."));
    }
    return;
  }

  const port = atlas.ports[s.port];
  body.appendChild(el("p", null, `${port.name}. ${port.economy}.`));

  // The one ship the chart can never draw. Your own mark is painted last and
  // covers whatever shares your hex, so a trader moored alongside you in a
  // harbour is invisible there by construction. Ports are where they gather,
  // so this is not a rare case, and it was the missing hull that made the
  // count in the status column look wrong.
  if (s.merchantHere) {
    body.appendChild(el("p", "dim",
      "A trader is moored alongside. The chandler here undersells her, but the guns are an option."));
  }

  // Standing, spelled out. The discount tops out at twelve percent, which is
  // small enough that a player would read it as noise in the price index
  // unless the figure is stated beside the market it applies to.
  const standing = s.favour === 0
    ? "A stranger here."
    : `Known here: ${s.favour} favour, ${s.favourDiscount}% off what they ask.`;
  const shut = s.stockedGoods - s.openGoods;
  const book = shut === 0
    ? " Their whole book is open to you."
    : ` ${shut} of their ${s.stockedGoods} goods are still closed to you.`;
  body.appendChild(el("p", "dim", standing + book));

  const wrap = el("div", "market--scroll");
  const table = el("table", "market");
  const head = el("tr");
  ["good", "hold", "buys at", "pays", ""].forEach((h) => head.appendChild(el("th", null, h)));
  table.appendChild(head);

  for (const row of s.market) {
    const tr = el("tr");
    tr.appendChild(el("td", null, atlas.goods[row.good]));
    tr.appendChild(el("td", null, row.have === 0 ? "·" : String(row.have)));
    // A shut good keeps its price and is marked shut. Blanking it would make
    // it indistinguishable from a good this economy never carried, and those
    // are opposite facts: one is an invitation to invest, the other is a
    // reason to sail somewhere else.
    const asks = row.buy < 0 ? "—"
      : row.shut ? `${row.buy} · shut`
      : String(row.buy);
    tr.appendChild(el("td", row.shut ? "dim" : null, asks));
    // A depressed price with no clock on it reads as a poor port rather than a
    // route you personally wore out, so the months are printed beside it.
    const pays = row.sell < 0 ? "—"
      : row.cool > 0 ? `${row.sell} · ${row.cool} mo` : String(row.sell);
    tr.appendChild(el("td", row.glut || row.cool > 0 ? "bad" : null, pays));

    const actions = el("td");
    if (row.buy >= 0 && !row.shut) {
      const b = el("button", null, "buy");
      b.id = `buy-${row.good}`;
      b.addEventListener("click", () => trade("buy", row.good));
      actions.appendChild(b);
    }
    if (row.have > 0) {
      const b = el("button", null, "sell");
      b.id = `sell-${row.good}`;
      b.addEventListener("click", () => trade("sell", row.good));
      actions.appendChild(b);
    }
    tr.appendChild(actions);
    table.appendChild(tr);
  }
  wrap.appendChild(table);
  body.appendChild(wrap);

  body.appendChild(qtyControl());

  // Every one of the seventy ports has these two, and only nine have a
  // shipyard. A harbour you could make and not victual out of would strand a
  // ship as surely as a hull that cannot reach the next market.
  body.appendChild(el("h3", "panel--sub", "the chandler"));
  chandler(s, body, false);

  body.appendChild(el("h3", "panel--sub", "the crimp"));
  const crimp = el("div", "helm--orders");
  const sign = el("button", null, `sign on hands, ${s.hireCost} each in advance`);
  sign.id = "crew-hire";
  if (s.crew >= s.crewMax) sign.setAttribute("aria-disabled", "true");
  sign.addEventListener("click", () => order(() => wasm.hire(currentQty)));
  crimp.appendChild(sign);
  const pay = el("button", null, "pay off hands");
  pay.id = "crew-discharge";
  if (s.crew <= 1) pay.setAttribute("aria-disabled", "true");
  pay.addEventListener("click", () => order(() => wasm.discharge(currentQty)));
  crimp.appendChild(pay);
  body.appendChild(crimp);
  body.appendChild(el("p", "dim",
    `${s.crew} aboard. She wants ${s.crewMin} to sail well and berths ${s.crewMax}. ` +
    `They eat ${s.foodPerDay} food and drink ${s.waterPerDay} water a day, ` +
    `and draw ${s.wages} gold at the turn of the month.`));

  body.appendChild(el("h3", "panel--sub", "the counting house"));
  const house = el("div", "helm--orders");
  const share = el("button", null,
    s.investCost < 0
      ? "nothing left to buy into"
      : `buy a share, ${s.investCost.toLocaleString("en")}`);
  share.id = "invest";
  // Never `disabled`, even with the book fully open: disabling the control the
  // player just pressed drops focus to <body>. It says why instead, and a
  // press when there is nothing left is a no-op the chronicle explains.
  if (s.investCost < 0) share.setAttribute("aria-disabled", "true");
  share.addEventListener("click", () => order(() => wasm.invest()));
  house.appendChild(share);
  body.appendChild(house);
  if (s.invested > 0) {
    body.appendChild(el("p", "dim",
      `${s.invested.toLocaleString("en")} sunk into this harbour so far.`));
  }

  drawShipyard(s, body);

  body.appendChild(el("h3", "panel--sub", "the shipwright"));
  const yard = el("div", "helm--orders");
  s.upgrades.forEach((u, i) => {
    // A tier at its ceiling reads differently from a tier you cannot afford,
    // and only the class can lift the first. Saying "as far as a Latina goes"
    // is the whole signpost back to the shipyard.
    const capped = u.tier >= u.ceiling;
    const b = el("button", null,
      capped
        ? `${u.name} at ${TIERS[u.tier]}, as far as a ${s.shipName} goes`
        : `${u.name} → ${TIERS[u.tier + 1]}, ${u.cost.toLocaleString("en")}`);
    b.id = `yard-${u.name}`;
    if (capped) b.setAttribute("aria-disabled", "true");
    b.addEventListener("click", () => order(() => wasm.upgrade(i)));
    yard.appendChild(b);
  });
  const fix = el("button", null,
    s.repairCost === 0 ? "sound, nothing to repair"
      : `shipwright: all of it for ${s.repairCost.toLocaleString("en")} gold and two days`);
  fix.id = "yard-repair";
  fix.addEventListener("click", () => order(() => wasm.repair()));
  yard.appendChild(fix);
  // Beside the paid repair on purpose: timber you already carry against gold
  // you would have to find is a choice, and it is only a choice if both are
  // in the same place. Sound, there is no choice to put, so the pair collapses
  // to the one line rather than showing two buttons that say the same nothing.
  if (s.damage > 0) yard.appendChild(mendButton(s));
  body.appendChild(yard);
}

/** The shipyard, at the nine capitals that have one. */
function drawShipyard(s, body) {
  if (!s.yard.length) return;
  body.appendChild(el("h3", "panel--sub", "the shipyard"));
  const list = el("ul", "helm--ships");
  // list-style: none removes list semantics in Safari with VoiceOver, so the
  // role goes back on explicitly. Same rule as .work--index on the home page.
  list.setAttribute("role", "list");
  for (const o of s.yard) {
    const li = el("li");
    const b = el("button");
    b.id = `ship-${o.class}`;
    // Never `disabled`. A locked hull is shown with its price and its reason,
    // the same bargain a shut good gets: a price you cannot meet is a goal,
    // and pressing it puts the reason in the chronicle.
    if (o.locked) b.setAttribute("aria-disabled", "true");
    b.appendChild(el("span", "ship--name", o.name));
    b.appendChild(el("span", "ship--price",
      o.price === 0 ? "no charge" : `${o.price.toLocaleString("en")} gold`));
    b.addEventListener("click", () => order(() => wasm.buy_ship(o.class)));
    li.appendChild(b);
    li.appendChild(el("p", "ship--blurb", o.blurb));
    li.appendChild(el("p", "dim",
      `${o.rig} · ${o.hold} in the hold · up to ${o.maxGuns} guns · ` +
      (o.bluewater ? "will cross an ocean" : "keeps the land in sight") +
      (o.deep ? " · draws too much for the shallowest harbours" : "")));
    if (o.locked) li.appendChild(el("p", "bad", o.locked));
    list.appendChild(li);
  }
  body.appendChild(list);
  if (s.yard[0] && s.yard[0].tradeIn > 0) {
    body.appendChild(el("p", "dim",
      `Prices are after ${s.yard[0].tradeIn.toLocaleString("en")} gold allowed against your ${s.shipName}.`));
  }
}

function drawKnown(s) {
  const list = $("known");
  list.replaceChildren();
  // Marked in the list rather than removed from it. A harbour she cannot enter
  // is still a harbour you know, still worth looking at, and still somewhere a
  // smaller hull would take you; dropping it would read as forgetting.
  const barred = new Set(s.barred);
  for (const i of s.known) {
    const port = atlas.ports[i];
    const li = el("li");
    li.appendChild(el("span", null,
      barred.has(i) ? `${port.name} (too shallow)` : port.name));

    const actions = el("span");
    const look = el("button", null, "look");
    look.id = `look-${i}`;
    look.addEventListener("click", () => lookAt(port.col, port.row));
    actions.appendChild(look);

    const sail = el("button", null, "course");
    sail.id = `course-${i}`;
    sail.addEventListener("click", () => order(() => wasm.set_course(i)));
    actions.appendChild(sail);

    li.appendChild(actions);
    list.appendChild(li);
  }
}

function drawChronicle(s) {
  const list = $("chronicle--lines");
  list.replaceChildren();
  // Newest first in the DOM; the stylesheet reverses it visually, so the live
  // region announces the new line rather than re-reading the whole log.
  for (let i = s.chronicle.length - 1; i >= 0; i--) {
    list.appendChild(el("li", null, s.chronicle[i]));
  }
}

/**
 * The "there" panel.
 *
 * Deliberately says what is *known*, not what is true. An unseen hex reports
 * that it is unseen, because the fog is the game and a panel that leaked
 * through it would give away the map.
 */
function drawThere() {
  const body = $("there--body");
  body.replaceChildren();
  if (!looking) {
    body.appendChild(el("p", "dim", "Click a hex, or pick a port below."));
    return;
  }

  wasm.write_look(looking.col, looking.row);
  const t = takeText();

  body.appendChild(el("p", null, latlon(t.col, t.row)));
  if (t.seen === 0) {
    body.appendChild(el("p", "dim", "Nothing is charted here."));
    return;
  }
  if (t.port >= 0) {
    body.appendChild(el("p", null, `${t.name}. ${t.economy}.`));
    // Said before the course button below it, not after, because the course is
    // the thing it is an answer to. Nothing is greyed out here, so the button
    // still offers itself and the log still refuses it; this is the warning
    // that stops that being the first the reader hears of it.
    if (t.barred) {
      body.appendChild(el("p", "dim",
        "Too shallow for her draught. A smaller hull could work in."));
    }
  } else {
    body.appendChild(el("p", null, t.land ? "Land." : "Open water."));
  }
  body.appendChild(el("p", "dim",
    `${t.distance} hexes off. ${t.seen === 2 ? "In sight." : "From memory; you are not looking at it now."}`));

  // Clicking the chart is "look there"; this is the "sail there" that the look
  // is usually a prelude to. It is a button rather than a second click on the
  // map because a course is a commitment and a stray click is not, and because
  // a reader who never touches the map still gets the same power from the port
  // list beneath. Offered for land and for the hex underfoot too, and refused
  // in the log with the reason, since nothing here is ever greyed out.
  const sail = el("button", null, "sail there");
  sail.id = "there--sail";
  sail.addEventListener("click", () => order(() => wasm.set_course_hex(t.col, t.row)));
  body.appendChild(sail);
}

function lookAt(col, row) {
  looking = { col, row };
  drawThere();
  const s = status();
  drawChart(s.col, s.row);
}

// -- the loop ---------------------------------------------------------------

let currentQty = 10;

function status() {
  wasm.write_status();
  return takeText();
}

/**
 * Run an order and redraw everything.
 *
 * Every order returns a number rather than throwing, and a refusal is written
 * into the chronicle by the Rust with a reason attached. That is why no control
 * on this page is ever disabled: the honest answer to "why can I not do that"
 * is a sentence, and a greyed-out button does not have one. It is also the
 * accessibility invariant in AGENTS.md, which this page keeps rather than
 * carves an exception out of.
 */
function order(run) {
  // Every panel is rebuilt from scratch on refresh, so the control that was
  // just pressed is a different element afterwards and focus would land on
  // <body>. That is the same failure the stable-`id` rule in AGENTS.md exists
  // to prevent for htmx, arrived at by a different road, and it is fixed the
  // same way: look the element back up by the id it kept. Buying stores is the
  // order that made this matter, because it is the first one a player presses
  // several times in a row.
  const had = document.activeElement && document.activeElement.id;
  run();
  refresh();
  if (had) {
    const again = $(had);
    if (again) again.focus();
  }
}

function refresh() {
  const s = status();
  drawChart(s.col, s.row);
  drawWho(s);
  drawHere(s);
  drawKnown(s);
  drawChronicle(s);
  drawThere();
}

function bind() {
  for (const button of document.querySelectorAll("[data-dir]")) {
    const dir = Number(button.dataset.dir);
    button.addEventListener("click", () => order(() => wasm.step(dir)));
  }
  $("order-on").addEventListener("click", () => order(() => wasm.sail_on()));
  $("order-wait").addEventListener("click", () => order(() => wasm.wait_here()));
  $("order-attack").addEventListener("click", () => order(() => wasm.attack()));
  $("order-new").addEventListener("click", () => {
    looking = null;
    order(() => wasm.init(Math.floor(Math.random() * 0xffffffff)));
  });

  document.addEventListener("keydown", (event) => {
    // Let the reader type a quantity, and leave the browser's own shortcuts be.
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const tag = document.activeElement && document.activeElement.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA") return;

    const key = event.key.toLowerCase();
    if (key in KEYS) {
      event.preventDefault();
      order(() => wasm.step(KEYS[key]));
    } else if (event.key === " ") {
      event.preventDefault();
      order(() => wasm.sail_on());
    } else if (event.key === ".") {
      event.preventDefault();
      order(() => wasm.wait_here());
    } else if (key === "f") {
      event.preventDefault();
      order(() => wasm.attack());
    }
  });
}

function trade(kind, good) {
  order(() => (kind === "buy" ? wasm.buy(good, currentQty) : wasm.sell(good, currentQty)));
}

async function boot() {
  let instance;
  try {
    // No import object: the crate imports nothing, which is checkable with
    // `wasm-objdump -x` and is the whole point of having no bindgen.
    const source = fetch(WASM_URL);
    if (WebAssembly.instantiateStreaming) {
      ({ instance } = await WebAssembly.instantiateStreaming(source));
    } else {
      const bytes = await (await source).arrayBuffer();
      ({ instance } = await WebAssembly.instantiate(bytes));
    }
  } catch (error) {
    $("boot").textContent =
      "The simulation would not load. It is a 60 kB WebAssembly module at " +
      WASM_URL + "; the browser reported: " + error;
    return;
  }

  wasm = instance.exports;
  wasm.init(Math.floor(Math.random() * 0xffffffff));

  wasm.write_atlas();
  atlas = takeText();
  cols = atlas.cols;
  rows = atlas.rows;

  buildChart();
  bind();
  $("boot").hidden = true;
  $("helm").hidden = false;
  refresh();
}

boot();
