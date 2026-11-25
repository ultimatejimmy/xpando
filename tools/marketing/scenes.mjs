// Records the screenshots and GIF frames for the website and wiki.
//   node scenes.mjs stills            all still screenshots, light and dark
//   node scenes.mjs gifs              all animations
//   node scenes.mjs <scene> [...]     named scenes, e.g. `create` or `still:library`
// Needs the app's dev server (`npm run dev -- --port 1421` in xpando-app). See README.md.
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, join } from "node:path";
import { session } from "./lib.mjs";
const HERE = dirname(fileURLToPath(import.meta.url));

const SCHEMES = ["light", "dark"];
const scrollDetail = (s, to = 99999) =>
  s.eval(`document.querySelectorAll(".detail, .detail *").forEach((e) => { if (e.scrollHeight > e.clientHeight + 20 && getComputedStyle(e).overflowY !== "visible") e.scrollTop = ${to}; })`);
const advancedOn = (s) => s.eval(`(() => { const c = document.querySelector('input[role=switch]'); if (!c.checked) c.click(); })()`);

// ----------------------------------------------------------------------------------------- stills
const STILLS = {
  async library(s) {
    await s.click(".side button, nav button, button", "Email");
    await s.click(".phrase", "Email signature");
    await s.hideCursor(); await s.wait(300);
  },
  async "editor-rich"(s) {
    await s.click("button", "Email");
    await s.click(".phrase", "Email signature");
    await s.click("button", "Edit"); await s.wait(500);
    await s.hideCursor();
  },
  async macros(s) {
    await s.click("button", "Email");
    await s.click(".phrase", "Out of office");
    await s.click("button", "Edit"); await s.wait(400);
    await s.click("button", "Macros"); await s.wait(400);
    await scrollDetail(s); await s.eval(`document.querySelectorAll(".detail, .detail *").forEach((e) => { if (e.scrollHeight > e.clientHeight + 20 && getComputedStyle(e).overflowY !== "visible") e.scrollTop -= 70; })`); await s.wait(300);
    await s.hideCursor();
  },
  async "macro-menu"(s) {
    await s.click("button", "Email");
    await s.click(".phrase", "Out of office");
    await s.click("button", "Edit"); await s.wait(400);
    await s.click("button", "Insert macro"); await s.wait(400);
  },
  async apps(s) {
    await s.click("button", "Outlook");
    await s.click(".phrase", "Meeting request");
    await s.hideCursor(); await s.wait(300);
  },
  async "works-in"(s) {
    await s.click("button", "Outlook");
    await s.click(".phrase", "Meeting request");
    await s.click("button", "Change"); await s.wait(700);
    await s.hideCursor();
  },
  async advanced(s) {
    await advancedOn(s); await s.wait(600);
    await s.click("button", "Email");
    await s.click(".phrase", "Email signature");
    await s.click("[role=tab]", "YAML"); await s.wait(700);
    await s.hideCursor();
  },
  async conflicts(s) {
    await advancedOn(s); await s.wait(600);
    await s.click("button", "trigger conflicts"); await s.wait(700);
    await s.hideCursor();
  },
  async search(s) {
    await s.click("input[type=search], .search input", undefined);
    await s.type("thank"); await s.wait(500);
    await s.click(".phrase", "Thanks for reaching out");
    await s.hideCursor();
  },
  async settings(s) {
    await s.click("button[aria-label*=ettings]"); await s.wait(700);
    await s.hideCursor();
  },
  async "new-phrase"(s) {
    await s.click("button", "Email");
    await s.click("button", "New phrase"); await s.wait(500);
    await s.click("input[aria-label='Trigger 1']"); await s.type(";intro2");
    await s.click("input.field:not(.mono)"); await s.type("Intro to a colleague");
    await s.click("textarea.field.mono"); await s.type("Hi {{name}},\n\nI'd like to introduce you to a colleague who can help.");
    await s.hideCursor();
  },
};

// ------------------------------------------------------------------------------------------ gifs
const GIFS = {
  /** Make a phrase from scratch, with a date macro, and watch it appear in the list. */
  async create(s) {
    await s.snap(900);
    await s.click("button", "Email", { record: true });
    await s.snap(300);
    await s.click("button", "New phrase", { record: true }); await s.wait(500); await s.snap(500);
    await s.click("input[aria-label='Trigger 1']", undefined, { record: true });
    await s.type(";fup", { record: true, per: 1, frameMs: 110 }); await s.snap(300);
    await s.click("input.field:not(.mono)", undefined, { record: true });
    await s.type("Friendly follow-up", { record: true, per: 3, frameMs: 70 }); await s.snap(250);
    await s.click("textarea.field.mono", undefined, { record: true });
    await s.type("Hi there,\n\nJust following up on my note from ", { record: true, per: 4, frameMs: 60 });
    await s.click("button", "Insert macro", { record: true }); await s.wait(350); await s.snap(700);
    await s.click("[role=menuitem]", "Date", { record: true }); await s.wait(500); await s.snap(1000);
    await s.hideCursor();
    await s.snap(900);
    await s.click("button", "Save", { record: true }); await s.wait(700);
    await s.hideCursor(); await s.snap(2200);
  },
  async search(s) {
    await s.snap(900);
    await s.click(".search input", undefined, { record: true });
    for (const q of ["t", "th", "tha", "than", "thank"]) {
      await s.eval(`(() => { const i = document.querySelector('.search input'); const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set; set.call(i, ${JSON.stringify(q)}); i.dispatchEvent(new Event('input',{bubbles:true})); })()`);
      await s.wait(220); await s.snap(q === "thank" ? 1100 : 280);
    }
    await s.click(".phrase", "Thanks for reaching out", { record: true }); await s.wait(300); await s.hideCursor(); await s.snap(1800);
  },

  async richtext(s) {
    await s.click("button", "Email");
    await s.click(".phrase", "Email signature");
    await s.snap(700);
    await s.click("button", "Edit", { record: true }); await s.wait(600); await s.snap(600);
    // click at the end of the last line, then add a new one
    const r = await s.find(".ProseMirror p:last-child");
    const cy = r.y + r.h - 9;
    await s.cursorTo(r.x + r.w - 40, cy, { steps: 8, snapEach: true, frameMs: 45 });
    await s.page.click(r.x + r.w - 40, cy); await s.wait(150);
    await s.press("End", "End", 35);
    await s.page.key("Enter", "Enter", { vk: 13, text: "\r" }); await s.wait(150); await s.snap(300);
    await s.click("button[aria-label='Bold']", undefined, { record: true });
    await s.type("Book a call: ", { record: true, per: 3, frameMs: 70 });
    await s.click("button[aria-label='Bold']", undefined, { record: true });
    await s.type("cal.northwind.example/alex", { record: true, per: 5, frameMs: 70 });
    await s.hideCursor(); await s.snap(1500);
  },
  async macros(s) {
    await s.click("button", "Email");
    await s.click(".phrase", "Out of office");
    await s.snap(700);
    await s.click("button", "Edit", { record: true }); await s.wait(500); await s.snap(500);
    await s.click("button", "Macros", { record: true }); await s.wait(400);
    await scrollDetail(s); await s.eval(`document.querySelectorAll(".detail, .detail *").forEach((e) => { if (e.scrollHeight > e.clientHeight + 20 && getComputedStyle(e).overflowY !== "visible") e.scrollTop -= 70; })`);
    await s.wait(300); await s.snap(1100);
    await s.click(".detail button", "November 24, 2026", { record: true }); await s.wait(300); await s.snap(1300);
    await s.click(".detail button", "Tuesday", { record: true }); await s.wait(300);
    await s.hideCursor(); await s.snap(1800);
  },
  async advanced(s) {
    await s.click("button", "Email");
    await s.click(".phrase", "Email signature");
    await s.snap(900);
    await s.click("input[role=switch], .adv-switch", undefined, { record: true }); await s.wait(500); await s.snap(1000);
    await s.click("[role=tab]", "YAML", { record: true }); await s.wait(500); await s.snap(1500);
    await s.click("button", "trigger conflicts", { record: true }); await s.wait(500);
    await s.hideCursor(); await s.snap(2200);
  },
  async worksin(s) {
    await s.click("button", "Email");
    await s.click(".phrase", "Meeting request");
    await s.snap(800);
    await s.click("button", "Change", { record: true }); await s.wait(700); await s.snap(900);
    await s.click("label", "Only these apps", { record: true }); await s.wait(300); await s.snap(400);
    await s.click("button, [role=option], li", "Word", { record: true }); await s.wait(400); await s.snap(1200);
    await s.click("button", "Move phrase", { record: true }); await s.wait(800);
    await s.hideCursor(); await s.snap(2000);
  },
  /** A staged illustration: type a trigger in any program and xpando swaps in the full phrase. */
  async expand(s) {
    const d = (js) => s.eval(`demo.${js}`);
    const typed = async (text, ms = 70) => { for (const ch of text) { await d(`type(${JSON.stringify(ch)})`); await s.snap(ms); } };
    await s.snap(900);
    await typed("Hi Dana,", 55); await d("type('\\n\\n')"); await s.snap(500);
    // first phrase: ;thx
    for (const ch of ";thx") { await d(`typeTrigger(${JSON.stringify(ch)})`); await s.snap(170); }
    await d("arm()"); await s.snap(450);
    await d(`expand(";thx", "Thanks so much for reaching out. I'll look into this today and get back to you by end of day.", true)`); await s.snap(1500);
    await d("settle()"); await d("type('\\n\\n')"); await d("hideToast()"); await s.snap(500);
    // second phrase: ;sig
    for (const ch of ";sig") { await d(`typeTrigger(${JSON.stringify(ch)})`); await s.snap(170); }
    await d("arm()"); await s.snap(450);
    await d(`expand(";sig", '<b>Alex Rivera</b><br>Customer Success Lead · Northwind Labs<br><a>northwind.example</a> · +1 (555) 013-2040', true)`); await s.snap(2800);
  },
};

// ----------------------------------------------------------------------------------------- runner
async function run() {
  const args = process.argv.slice(2);
  const want = args.length ? args : ["stills", "gifs"];
  const names = [];
  for (const a of want) {
    if (a === "stills") names.push(...Object.keys(STILLS).map((n) => "still:" + n));
    else if (a === "gifs") names.push(...Object.keys(GIFS).map((n) => "gif:" + n));
    else if (a.includes(":")) names.push(a);
    else names.push(STILLS[a] ? "still:" + a : "gif:" + a);
  }
  for (const n of names) {
    const [kind, id] = n.split(":");
    for (const scheme of kind === "gif" ? ["light"] : SCHEMES) {
      const opts = kind === "gif" && id === "expand" ? { url: pathToFileURL(join(HERE, "expand.html")).href, mock: false, width: 1100, height: 540 } : {};
      const s = await session({ scheme, scale: 2, ...opts });
      try {
        if (kind === "still") { await STILLS[id](s); await s.still(`${id}.${scheme}`); }
        else { await GIFS[id](s); s.save(`${id}.${scheme}`); }
      } finally { await s.close(); }
    }
  }
}
run().catch((e) => { console.error(e); process.exit(1); });
