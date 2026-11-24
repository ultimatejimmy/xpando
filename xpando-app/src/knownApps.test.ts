// Run: node --test --experimental-strip-types src/knownApps.test.ts
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  KNOWN_APPS,
  addExes,
  chipsFor,
  exeKey,
  filterRows,
  isWindows,
  knownFor,
  knownRows,
  normalizeExe,
  parseTypedExes,
  programRows,
  removeExes,
  rowSelected,
  toggleRow,
} from "./knownApps.ts";

test("curated list: every entry is complete and nothing is listed twice", () => {
  assert.ok(KNOWN_APPS.length >= 30);
  const names = new Set<string>();
  const keys = new Map<string, string>();
  for (const app of KNOWN_APPS) {
    assert.ok(app.name.trim() === app.name && app.name.length > 0, `name ${app.name}`);
    assert.ok(!names.has(app.name.toLowerCase()), `duplicate name ${app.name}`);
    names.add(app.name.toLowerCase());
    assert.ok(app.exes.length > 0, `${app.name} has no program`);
    for (const exe of app.exes) {
      assert.match(exe, /\.exe$/, `${app.name}: ${exe} should end in .exe`);
      assert.equal(exe, exe.toLowerCase(), `${exe} should be lower case`);
      assert.equal(normalizeExe(exe), exe, `${exe} must be accepted as typed`);
      const k = exeKey(exe);
      assert.ok(!keys.has(k), `${exe} is listed for both ${keys.get(k)} and ${app.name}`);
      keys.set(k, app.name);
    }
  }
});

test("curated list: the programs the user asked for are there", () => {
  const want: [string, string][] = [
    ["Word", "winword.exe"],
    ["Excel", "excel.exe"],
    ["PowerPoint", "powerpnt.exe"],
    ["Outlook", "outlook.exe"],
    ["Outlook (new)", "olk.exe"],
    ["OneNote", "onenote.exe"],
    ["Chrome", "chrome.exe"],
    ["Edge", "msedge.exe"],
    ["Firefox", "firefox.exe"],
    ["Brave", "brave.exe"],
    ["Teams", "ms-teams.exe"],
    ["Teams", "teams.exe"],
    ["Slack", "slack.exe"],
    ["Discord", "discord.exe"],
    ["Notepad", "notepad.exe"],
    ["Notepad++", "notepad++.exe"],
    ["Visual Studio Code", "code.exe"],
    ["Windows Terminal", "windowsterminal.exe"],
    ["PowerShell", "powershell.exe"],
    ["Command Prompt", "cmd.exe"],
    ["File Explorer", "explorer.exe"],
    ["Zoom", "zoom.exe"],
    ["Claude", "claude.exe"],
  ];
  for (const [name, exe] of want) assert.equal(knownFor(exe)?.name, name, exe);
  assert.ok(KNOWN_APPS.some((a) => a.name.startsWith("Adobe Acrobat")));
});

test("exeKey ignores case, spaces and .exe", () => {
  assert.equal(exeKey(" WinWord.EXE "), "winword");
  assert.equal(exeKey("notepad++"), "notepad++");
  assert.equal(exeKey(".exe"), "");
});

test("normalizeExe takes the file name out of what was typed or pasted", () => {
  assert.equal(normalizeExe("winword.exe"), "winword.exe");
  assert.equal(normalizeExe("  WINWORD.EXE "), "WINWORD.EXE");
  assert.equal(normalizeExe('"C:\\Program Files\\Microsoft Office\\root\\Office16\\WINWORD.EXE"'), "WINWORD.EXE");
  assert.equal(normalizeExe("/Applications/Safari.app/Contents/MacOS/Safari"), "Safari");
  assert.equal(normalizeExe("Cloudflare WARP.exe"), "Cloudflare WARP.exe");
  assert.equal(normalizeExe("notepad++.exe"), "notepad++.exe");
  for (const bad of ["", "   ", ".exe", "a;b.exe", "a|b", "x*.exe", "C:\\folder\\", "a".repeat(80) + ".exe"]) {
    assert.equal(normalizeExe(bad), null, JSON.stringify(bad));
  }
});

test("addExes and removeExes ignore case and .exe", () => {
  assert.deepEqual(addExes(["winword.exe"], ["WINWORD", "excel.exe", "Excel.EXE"]), ["winword.exe", "excel.exe"]);
  assert.deepEqual(addExes([], []), []);
  assert.deepEqual(removeExes(["winword.exe", "excel.exe"], ["WINWORD.EXE"]), ["excel.exe"]);
  const original = ["a.exe"];
  addExes(original, ["b.exe"]);
  assert.deepEqual(original, ["a.exe"], "input must not change");
});

test("parseTypedExes splits, cleans, de-duplicates and reports rejects", () => {
  const r = parseTypedExes('winword.exe, "C:\\x\\EXCEL.EXE"; winword\nbad*name|x');
  assert.deepEqual(r.exes, ["winword.exe", "EXCEL.EXE"]);
  assert.deepEqual(r.rejected, ["bad*name|x"]);
});

test("chips: curated programs group under their friendly name", () => {
  const chips = chipsFor(["ms-teams.exe", "winword.exe", "teams.exe", "custom.exe"]);
  assert.deepEqual(chips, [
    { label: "Teams", exes: ["ms-teams.exe", "teams.exe"] },
    { label: "Word", exes: ["winword.exe"] },
    { label: "custom", exes: ["custom.exe"] },
  ]);
  // Only one of two builds selected: the chip removes just that one.
  assert.deepEqual(chipsFor(["teams.exe"]), [{ label: "Teams", exes: ["teams.exe"] }]);
});

test("chips: other programs use the name the app reported, else the file name", () => {
  const seen = [
    { exe: "SDM.exe", name: "Software Deployment Manager" },
    { exe: "plain.exe", name: "plain" },
  ];
  assert.deepEqual(chipsFor(["sdm.EXE", "plain.exe", "other.exe"], seen), [
    { label: "Software Deployment Manager", exes: ["sdm.EXE"] },
    { label: "plain", exes: ["plain.exe"] },
    { label: "other", exes: ["other.exe"] },
  ]);
});

test("rows: reported programs are de-duplicated and curated names win", () => {
  const rows = programRows([
    { exe: "WINWORD.EXE", name: "Microsoft Office Word" },
    { exe: "winword.exe", name: "Word again" },
    { exe: "zed.exe", name: "Zed Editor" },
    { exe: "abc.exe", name: "" },
  ]);
  assert.deepEqual(
    rows.map((r) => [r.name, r.exes[0]]),
    [
      ["abc", "abc.exe"],
      ["Word", "WINWORD.EXE"],
      ["Zed Editor", "zed.exe"],
    ],
  );
});

test("rows: search matches friendly names and file names, every word", () => {
  const rows = knownRows();
  assert.deepEqual(filterRows(rows, "word").map((r) => r.name), ["Word", "WordPad"]);
  assert.ok(filterRows(rows, "WINWORD").some((r) => r.name === "Word"));
  assert.ok(filterRows(rows, "visual code").some((r) => r.name === "Visual Studio Code"));
  assert.equal(filterRows(rows, "zzzz").length, 0);
  assert.equal(filterRows(rows, "  ").length, rows.length);
});

test("toggling a row selects or removes all of its programs", () => {
  const teams = knownRows().find((r) => r.name === "Teams")!;
  const on = toggleRow(["excel.exe"], teams);
  assert.deepEqual(on, ["excel.exe", "ms-teams.exe", "teams.exe"]);
  assert.ok(rowSelected(teams, on));
  assert.deepEqual(toggleRow(on, teams), ["excel.exe"]);
  assert.ok(!rowSelected(teams, ["teams.exe"]));
});

test("isWindows reads the user agent", () => {
  assert.ok(isWindows("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"));
  assert.ok(!isWindows("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)"));
});
