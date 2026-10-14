// Execute the self-contained page's real script and event handlers against a
// minimal DOM model. This verifies wiring, not browser layout or accessibility.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
class Element {
  constructor() {
    this.children = [];
    this.textContent = "";
    this.disabled = false;
  }
  append(...children) {
    this.children.push(...children);
  }
  replaceChildren(...children) {
    this.children = children;
  }
}
const elements = new Map();
const document = {
  getElementById(id) {
    if (!elements.has(id)) elements.set(id, new Element());
    return elements.get(id);
  },
  createElement() {
    return new Element();
  },
  createTextNode(value) {
    return { textContent: value };
  },
  querySelector(selector) {
    return this.getElementById(selector.slice(1));
  },
};
const html = fs.readFileSync(path.join(__dirname, "index.html"), "utf8");
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
const context = vm.createContext({ document, console: { log() {} } });
vm.runInContext(script, context, { timeout: 5000 });
const el = (id) => document.getElementById(id);
assert.match(el("tests").textContent, /35 scenario assertions passed/);
assert.equal(el("answer").textContent, "pending");
const tabs = [...el("tabs").children];
for (const tab of tabs) {
  tab.onclick();
  const stepCount = el("steps").children.length;
  for (let i = 0; i < stepCount; i++) {
    const button = el("steps").children[i];
    assert.equal(button.disabled, false);
    button.onclick();
  }
  if (tab.textContent === "Forward declaration")
    assert.equal(el("answer").textContent, "pending");
  if (tab.textContent === "Duplicate names")
    assert.match(el("answer").textContent, /^Conflict:/);
  if (tab.textContent === "Unknown producer keys")
    assert.equal(el("answer").textContent, "pending");
  if (tab.textContent === "Producer discovery")
    assert.equal(el("answer").textContent, "Present: outer");
  if (tab.textContent === "Circular dependency")
    assert.equal(el("answer").textContent, "pending");
  if (tab.textContent === "Hypothetical types")
    assert.match(
      el("trial").textContent,
      /return: string; original remains number/,
    );
  el("reset").onclick();
  assert.equal(el("steps").children[0].disabled, false);
}
tabs[0].onclick();
for (const button of el("free").children) button.onclick();
assert.equal(el("answer").textContent, "pending");
console.log(
  `${tabs.length} guided walkthroughs and free-play/reset handlers passed (DOM model; no visual verification).`,
);
