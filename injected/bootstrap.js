(() => {
  "use strict";
  const key = "__HARNESS_SESSION_PLUS__";
  if (window[key]?.version === "0.1.1") return window[key].install();
  if (window[key]?.version === "0.1.0" && typeof window[key].uninstall === "function") window[key].uninstall();
  if (window[key]) throw new Error("Unknown HSP installation; refusing overwrite");
  let observer, timer, style, installed = false;
  const buttons = new Set();
  const handlers = new Map();
  function scan() {
    timer = undefined;
    if (!installed) return;
    for (const button of buttons) {
      if (!button.isConnected) { button.removeEventListener("click", handlers.get(button)); handlers.delete(button); buttons.delete(button); }
    }
    document.querySelectorAll('[role="menu"]').forEach(menu => {
      const native = Array.from(menu.querySelectorAll('[role="menuitem"],button')).find(el =>
        el.textContent?.trim() === "归档会话" ||
        Array.from(el.querySelectorAll('span')).some(label => label.textContent?.trim() === "归档会话")
      );
      if (!native || menu.querySelector('[data-hsp-test]')) return;
      const button = document.createElement("button");
      button.type = "button";
      button.setAttribute("role", "menuitem");
      button.dataset.hspTest = "true";
      button.className = "hsp-test-action";
      button.textContent = "回收站 · 连接测试";
      const handler = event => { event.preventDefault(); event.stopPropagation(); window.alert("Harness Session Plus 已连接。此按钮仅用于测试，不修改或删除会话。"); };
      button.addEventListener("click", handler);
      handlers.set(button, handler); buttons.add(button); (native.parentElement || menu).append(button);
    });
  }
  function schedule() { if (timer === undefined) timer = window.setTimeout(scan, 80); }
  const api = {
    version: "0.1.1",
    install() {
      if (installed) return api.getStatus();
      if (!document.documentElement) throw new Error("Document not ready");
      installed = true;
      style = document.createElement("style"); style.dataset.hspStyle = "true";
      style.textContent = '.hsp-test-action{display:flex;align-items:center;width:100%;padding:8px 12px;border:0;border-radius:6px;background:transparent;color:inherit;font:inherit;text-align:left;cursor:pointer;transition:background-color 140ms ease}.hsp-test-action:hover{background:color-mix(in srgb,currentColor 9%,transparent)}.hsp-test-action:focus-visible{outline:2px solid hsl(232 88% 64%);outline-offset:-2px}';
      document.documentElement.append(style);
      observer = new MutationObserver(records => {
        if (records.some(record => !record.target.closest?.('[data-hsp-test],[data-hsp-style]'))) schedule();
      });
      observer.observe(document.documentElement, {childList: true, subtree: true});
      scan(); return api.getStatus();
    },
    getStatus() { return {installed, version: api.version, testButtons: buttons.size, dataOperations: false}; },
    uninstall() {
      installed = false; observer?.disconnect(); observer = undefined;
      if (timer !== undefined) window.clearTimeout(timer); timer = undefined;
      for (const button of buttons) { button.removeEventListener("click", handlers.get(button)); button.remove(); }
      buttons.clear(); handlers.clear(); style?.remove(); style = undefined;
      delete window[key]; return {installed: false, dataOperations: false};
    }
  };
  Object.defineProperty(window, key, {value: api, configurable: true});
  return api.install();
})()
