// A tiny Chrome DevTools Protocol driver with no dependencies (Node 22+ has WebSocket built in).
// It starts a headless Chrome/Edge of its own, so it never touches your desktop or your browser.
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const CANDIDATES = [
  process.env.CHROME_PATH,
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  "/usr/bin/google-chrome",
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
].filter(Boolean);

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function launch({ width = 1280, height = 800, scale = 1, port = 9333 } = {}) {
  const exe = CANDIDATES.find((c) => existsSync(c));
  if (!exe) throw new Error("No Chrome or Edge found; set CHROME_PATH");
  const dir = mkdtempSync(join(tmpdir(), "xpando-cap-"));
  const proc = spawn(exe, [
    "--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${dir}`,
    `--window-size=${width},${height}`, "--hide-scrollbars", "--no-first-run", "--disable-gpu",
    "--force-color-profile=srgb", "--disable-extensions", "--no-default-browser-check", "about:blank",
  ], { stdio: "ignore" });
  let version;
  for (let i = 0; i < 60; i++) {
    try { version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json(); break; } catch { await sleep(250); }
  }
  if (!version) throw new Error("Chrome did not start");
  const tabs = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  const tab = tabs.find((t) => t.type === "page");
  const ws = new WebSocket(tab.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
  let id = 0; const waiting = new Map();
  ws.onmessage = (m) => { const d = JSON.parse(m.data); if (d.id && waiting.has(d.id)) { const { res, rej } = waiting.get(d.id); waiting.delete(d.id); d.error ? rej(new Error(d.error.message)) : res(d.result); } };
  const send = (method, params = {}) => new Promise((res, rej) => { const i = ++id; waiting.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params })); });

  await send("Page.enable");
  await send("Runtime.enable");
  await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: scale, mobile: false });

  const page = {
    send, width, height, scale,
    async init(script) { await send("Page.addScriptToEvaluateOnNewDocument", { source: script }); },
    async goto(url) { await send("Page.navigate", { url }); await sleep(900); },
    async eval(expr) {
      const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text);
      return r.result.value;
    },
    async shot(clip) {
      const r = await send("Page.captureScreenshot", { format: "png", ...(clip ? { clip: { ...clip, scale: 1 } } : {}) });
      return Buffer.from(r.data, "base64");
    },
    async rect(selector, text) {
      return page.eval(`(() => { const els = [...document.querySelectorAll(${JSON.stringify(selector)})];
        const el = ${text ? `els.find(e => e.textContent.trim().includes(${JSON.stringify(text)}))` : "els[0]"};
        if (!el) return null; el.scrollIntoView({block:'nearest'}); const r = el.getBoundingClientRect();
        return { x: r.x, y: r.y, w: r.width, h: r.height, cx: r.x + r.width / 2, cy: r.y + r.height / 2 }; })()`);
    },
    async mouse(type, x, y, extra = {}) { await send("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1, ...extra }); },
    async click(x, y) { await page.mouse("mouseMoved", x, y); await page.mouse("mousePressed", x, y); await page.mouse("mouseReleased", x, y); },
    async key(key, code, extra = {}) {
      await send("Input.dispatchKeyEvent", { type: "keyDown", key, code: code || key, windowsVirtualKeyCode: extra.vk || 0, ...extra });
      await send("Input.dispatchKeyEvent", { type: "keyUp", key, code: code || key, windowsVirtualKeyCode: extra.vk || 0 });
    },
    async insert(text) { await send("Input.insertText", { text }); },
    async close() { try { ws.close(); } catch {} proc.kill(); await sleep(400); try { rmSync(dir, { recursive: true, force: true }); } catch {} },
  };
  return page;
}
