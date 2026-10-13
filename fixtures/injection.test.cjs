// Node is a test runner only; the product has no Node service or runtime dependency.
const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');
const source = fs.readFileSync(path.join(__dirname, '../injected/bootstrap.js'), 'utf8');
function environment(options = {}) {
  let observer, alerts = 0;
  const timers = new Map();
  const nodes = [];
  class Element {
    constructor(tag, text = '') { this.tag = tag; this.textContent = text; this.dataset = {}; this.listeners = new Map(); this.isConnected = true; }
    setAttribute(name, value) { this[name] = value; }
    addEventListener(name, handler) { this.listeners.set(name, handler); }
    removeEventListener(name, handler) { assert.equal(this.listeners.get(name), handler); this.listeners.delete(name); }
    append(child) { nodes.push(child); }
    remove() { this.isConnected = false; }
    querySelectorAll(selector) {
      if (selector === 'span') return options.shortcut ? [new Element('span', '归档会话'), new Element('span', 'Ctrl + Shift + A')] : [];
      const native = new Element('button', options.shortcut ? '归档会话 Ctrl + Shift + A' : '归档会话');
      native.parentElement = menu;
      return [native];
    }
    querySelector() { return nodes.find(n => n.isConnected && n.dataset.hspTest); }
  }
  const menu = new Element('menu');
  const document = { documentElement: new Element('html'), createElement: tag => new Element(tag), querySelectorAll: () => [menu] };
  const window = {alert: () => alerts++, setTimeout: fn => { const id = timers.size + 1; timers.set(id, fn); return id; }, clearTimeout: id => timers.delete(id)};
  class MutationObserver {
    constructor(callback) { this.callback = callback; observer = this; }
    observe() { this.active = true; }
    disconnect() { this.active = false; }
  }
  const context = vm.createContext({window, document, MutationObserver});
  return {window, document, nodes, timers, get observer() {return observer;}, get alerts() {return alerts;}, run: () => vm.runInContext(source, context)};
}
test('reinstall is idempotent and click is alert-only', () => {
  const env = environment();
  assert.equal(env.run().installed, true);
  assert.equal(env.run().testButtons, 1);
  assert.equal(env.nodes.filter(n => n.dataset.hspTest).length, 1);
  const button = env.nodes.find(n => n.dataset.hspTest);
  let prevented = 0;
  button.listeners.get('click')({preventDefault: () => prevented++,stopPropagation: () => prevented++});
  assert.equal(env.alerts, 1); assert.equal(prevented, 2);
  assert.equal(env.window.__HARNESS_SESSION_PLUS__.getStatus().dataOperations, false);
});
test('uninstall cleans buttons, styles, listener, observer and pending timer; fresh install works', () => {
  const env = environment(); env.run();
  env.observer.callback([{target: {}}]); env.observer.callback([{target: {}}]);
  assert.equal(env.timers.size, 1);
  const prior = env.nodes.slice();
  assert.equal(env.window.__HARNESS_SESSION_PLUS__.uninstall().installed, false);
  assert.equal(env.observer.active, false); assert.equal(env.timers.size, 0);
  assert.equal(env.window.__HARNESS_SESSION_PLUS__, undefined);
  assert(prior.every(n => !n.isConnected && n.listeners.size === 0));
  assert.equal(env.run().testButtons, 1);
});
test('unknown existing installation is never overwritten', () => {
  const env = environment(); const foreign = {version: 'unknown'};
  env.window.__HARNESS_SESSION_PLUS__ = foreign;
  assert.throws(() => env.run(), /refusing overwrite/);
  assert.equal(env.window.__HARNESS_SESSION_PLUS__, foreign);
  assert.equal(env.nodes.length, 0);
});
test('archive label with a keyboard shortcut still gets exactly one test button', () => {
  const env = environment({shortcut:true});
  assert.equal(env.run().testButtons,1);
  assert.equal(env.run().testButtons,1);
  assert.equal(env.nodes.filter(n=>n.dataset.hspTest).length,1);
  env.window.__HARNESS_SESSION_PLUS__.uninstall();
  assert(env.nodes.every(n=>!n.isConnected));
});
test('known 0.1.0 installation is uninstalled before installing the fix', () => {
  const env=environment({shortcut:true});let removed=0;
  env.window.__HARNESS_SESSION_PLUS__={version:'0.1.0',uninstall(){removed++;delete env.window.__HARNESS_SESSION_PLUS__;}};
  assert.equal(env.run().version,'0.1.1');assert.equal(removed,1);assert.equal(env.run().testButtons,1);
});
