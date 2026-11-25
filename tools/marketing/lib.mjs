// Shared helpers for the capture scenes: start the app with the demo backend, drive it like a
// person would, and record frames.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { launch, sleep } from "./cdp.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
export const OUT = join(HERE, "out", "raw");
export const APP_URL = process.env.XPANDO_DEV_URL || "http://localhost:1421/";

// A pointer drawn into the page, since a headless browser has none of its own.
const CURSOR_JS = `(() => {
  const mk = () => {
    if (document.getElementById("__cur") || !document.documentElement) return;
    const d = document.createElement("div");
    d.id = "__cur";
    d.style.cssText = "position:fixed;left:0;top:0;width:24px;height:24px;z-index:2147483647;pointer-events:none;transform:translate(-100px,-100px);transition:none;filter:drop-shadow(0 1px 1.5px rgba(0,0,0,.45))";
    d.innerHTML = '<svg width="24" height="24" viewBox="0 0 24 24"><path d="M4 2.5v17l4.6-4.1 3.1 6.9 2.9-1.3-3.1-6.8h6.3z" fill="#fff" stroke="#111" stroke-width="1.5" stroke-linejoin="round"/></svg>';
    document.documentElement.appendChild(d);
  };
  document.addEventListener("DOMContentLoaded", mk);
  window.__cursor = (x, y, down) => { mk(); const c = document.getElementById("__cur"); c.style.transform = "translate(" + x + "px," + y + "px) scale(" + (down ? 0.88 : 1) + ")"; };
})();`;

export async function session({ scheme = "light", width = 1280, height = 800, scale = 1, side = 250, list = 380, advanced = false, keepLocal = {}, url = APP_URL, mock = true } = {}) {
  const page = await launch({ width, height, scale });
  await page.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }] });
  const store = { "xpando.sideWidth": String(side), "xpando.listWidth": String(list), ...keepLocal };
  const seed = `try { ${Object.entries(store).map(([k, v]) => `localStorage.setItem(${JSON.stringify(k)}, ${JSON.stringify(v)});`).join(" ")} ${advanced ? "" : ""} } catch {}`;
  await page.init(seed);
  if (mock) await page.init(readFileSync(join(HERE, "mock.js"), "utf8"));
  await page.init(CURSOR_JS);
  await page.goto(url);
  await page.eval("document.fonts.ready.then(() => true)");
  await sleep(1200);
  return new Scene(page);
}

export class Scene {
  constructor(page) { this.page = page; this.frames = []; this.cx = 640; this.cy = 400; }

  async wait(ms = 250) { await sleep(ms); }

  /** Finds an element by CSS selector and optional text; throws if it is not on screen. */
  async find(selector, text) {
    const r = await this.page.rect(selector, text);
    if (!r) throw new Error(`not found: ${selector} ${text ?? ""}`);
    return r;
  }

  async cursorTo(x, y, { steps = 8, ms = 20, snapEach = false, frameMs = 40, down = false } = {}) {
    const x0 = this.cx, y0 = this.cy;
    for (let i = 1; i <= steps; i++) {
      const t = i / steps, e = t < 0.5 ? 2 * t * t : 1 - Math.pow(-2 * t + 2, 2) / 2; // ease in-out
      this.cx = x0 + (x - x0) * e; this.cy = y0 + (y - y0) * e;
      await this.page.eval(`window.__cursor(${this.cx},${this.cy},${down})`);
      if (snapEach) await this.snap(frameMs);
      else await sleep(ms);
    }
  }

  /** Moves the pointer to an element and clicks it. With `record`, the motion becomes frames. */
  async click(selector, text, { record = false, steps = 9, offset } = {}) {
    const r = await this.find(selector, text);
    const x = offset ? r.x + offset[0] : r.cx, y = offset ? r.y + offset[1] : r.cy;
    await this.cursorTo(x, y, { steps, snapEach: record, frameMs: 45 });
    await this.page.eval(`window.__cursor(${x},${y},true)`);
    if (record) await this.snap(70);
    await this.page.click(x, y);
    await this.page.eval(`window.__cursor(${x},${y},false)`);
    await sleep(220);
  }

  /** Types text into whatever has focus, a few characters per frame. */
  async type(text, { record = false, per = 1, frameMs = 60 } = {}) {
    const chars = [...text];
    for (let i = 0; i < chars.length; i += per) {
      await this.page.insert(chars.slice(i, i + per).join(""));
      await sleep(40);
      if (record) await this.snap(frameMs);
    }
  }

  async press(key, code, vk) { await this.page.key(key, code, { vk }); await sleep(120); }

  hideCursor() { return this.page.eval("window.__cursor(-100,-100,false)"); }

  /** Records the current screen as a frame that shows for `ms` milliseconds. */
  async snap(ms = 100) { this.frames.push({ png: await this.page.shot(), ms }); }

  /** Saves a single still image. */
  async still(name) {
    mkdirSync(OUT, { recursive: true });
    writeFileSync(join(OUT, `${name}.png`), await this.page.shot());
    console.log("still", name);
  }

  /** Saves the recorded frames and a manifest the compositor reads. */
  save(name) {
    const dir = join(OUT, name);
    mkdirSync(dir, { recursive: true });
    this.frames.forEach((f, i) => writeFileSync(join(dir, String(i).padStart(4, "0") + ".png"), f.png));
    writeFileSync(join(dir, "frames.json"), JSON.stringify(this.frames.map((f) => f.ms)));
    console.log("frames", name, this.frames.length);
    this.frames = [];
  }

  eval(js) { return this.page.eval(js); }
  close() { return this.page.close(); }
}
