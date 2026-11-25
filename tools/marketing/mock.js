// A stand-in for the Tauri backend, so the real xpando front end can run in a plain browser
// with a made-up phrase library. Used only to make the screenshots and GIFs for the website
// and wiki (see README.md). It is injected before the page loads; nothing here ships in the app.
(() => {
  const NO_OPTS = () => ({ word: null, left_word: null, right_word: null, propagate_case: null, uppercase_style: null, force_mode: null, search_terms: [], folder: null });
  const strip = (h) => h.replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim();
  const date = (name, fmt, extra = []) => ({ name, kind: "date", params: [["format", fmt], ...extra] });
  const clip = (name) => ({ name, kind: "clipboard", params: [] });

  const p = (triggers, label, content, o = {}) => ({
    triggers, is_regex: false, label, kind: o.kind || "replace", content,
    preview: o.kind === "html" ? strip(content) : content.replace(/\s+/g, " "),
    vars: o.vars || [], comment: o.comment || null, index: 0, line: null,
    options: { ...NO_OPTS(), folder: o.folder ?? null, word: o.word ?? null, search_terms: o.terms || [] },
  });

  const SIG = `<p><strong>Alex Rivera</strong><br>Customer Success Lead · Northwind Labs<br><a href="https://northwind.example">northwind.example</a> · +1 (555) 013-2040</p>`;
  const files = {
    "base.yml": { name: "Base", phrases: [
      p([";sig"], "Email signature", SIG, { kind: "html", folder: "Email", comment: "Rich text, with a link and line breaks." }),
      p([";thx"], "Thanks for reaching out", "Hi {{name}},\n\nThanks so much for reaching out. I'll look into this today and get back to you by end of day.\n\nBest,\nAlex", { folder: "Email", vars: [clip("name")] }),
      p([";ooo"], "Out of office", "Hi! I'm out of the office until {{back}} with limited access to email. For anything urgent, please contact support@northwind.example.\n\nAlex", { folder: "Email", vars: [date("back", "%A, %B %e", [["offset", "432000"]])] }),
      p([";intro"], "Warm introduction", "Hi {{name}},\n\nI wanted to introduce you to a colleague who can help with this. I'll let you two take it from here.\n\nThanks,\nAlex", { folder: "Email", vars: [clip("name")] }),
      p([";today"], "Today's date", "{{d}}", { folder: "Dates", vars: [date("d", "%A, %B %e, %Y")] }),
      p([";now"], "Timestamp", "{{ts}}", { folder: "Dates", vars: [date("ts", "%Y-%m-%d %H:%M")] }),
      p([";fri"], "Next Friday", "{{d}}", { folder: "Dates", vars: [date("d", "%a %b %e", [["offset", "259200"]])] }),
      p([";addr"], "Home address", "1412 Maple Street\nPortland, OR 97205", { folder: "Personal" }),
      p([";ad"], "Short address", "1412 Maple St", { folder: "Personal", comment: "Shares its start with ;addr, which xpando flags." }),
      p([";em"], "Personal email", "alex.rivera@example.com", { folder: "Personal" }),
      p([";ph"], "Phone number", "+1 (555) 013-2040", { folder: "Personal" }),
      p([";sorry"], "Apology reply", `<p>Hi there,</p><p>I'm really sorry for the trouble. That's not the experience we want you to have, and I'd like to make it right.</p><ul><li>I've escalated this to our engineers</li><li>You'll hear from me again <strong>within 24 hours</strong></li></ul><p>Thanks for your patience,<br>Alex</p>`, { kind: "html", folder: "Support" }),
      p([";reset"], "Password reset steps", "No problem! To reset your password:\n\n1. Go to northwind.example/login\n2. Choose “Forgot password”\n3. Check your email for the link (it expires in 30 minutes)", { folder: "Support" }),
      p([";kb"], "Knowledge base link", "You can find a step-by-step guide here: https://help.northwind.example/getting-started", { folder: "Support", terms: ["docs", "help"] }),
      p([";status"], "Project status table", `<p>Weekly status</p><table><tbody><tr><th>Area</th><th>Status</th><th>Owner</th></tr><tr><td>Onboarding</td><td>On track</td><td>Alex</td></tr><tr><td>Billing</td><td>At risk</td><td>Priya</td></tr></tbody></table>`, { kind: "html", folder: "Snippets" }),
      p([";greet"], "Random greeting", "{{g}}", { folder: "Snippets", vars: [{ name: "g", kind: "random", params: [["choices", "Hi there!|Hello!|Hey, good to hear from you!|Greetings!"]] }] }),
      p([";mtg"], "Meeting link", "{{link}}", { folder: "Snippets", vars: [{ name: "link", kind: "choice", params: [["values", "https://meet.example.com/standup|https://meet.example.com/office-hours|https://meet.example.com/1on1"]] }] }),
      p([";lorem"], "Lorem ipsum", "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.", { folder: "Snippets" }),
      p([";ty"], "Thank you", "Thank you!", { folder: "Email", comment: "Shares its start with ;tyvm, which xpando flags." }),
      p([";tyvm"], "Thank you very much", "Thank you very much! I really appreciate it.", { folder: "Email" }),
    ] },
    "outlook.yml": { name: "Outlook", phrases: [
      p([";meet"], "Meeting request", "Hi {{name}},\n\nDo you have 20 minutes this week to connect? Here are a few times that work for me:\n\n• Tuesday 10:00\n• Wednesday 14:30\n• Thursday 09:00", { folder: "Email", vars: [clip("name")] }),
      p([";fup"], "Follow-up", "Following up on my note from {{d}}. Any update on your side?", { folder: "Email", vars: [date("d", "%A")] }),
    ] },
    "code.yml": { name: "Visual Studio Code", phrases: [
      p([";clog"], "console.log", "console.log('$|$');", { folder: "Snippets" }),
      p([";fn"], "Arrow function", "const name = (args) => {\n  \n};", { folder: "Snippets" }),
      p([";todo"], "TODO comment", "// TODO({{who}}): ", { folder: "Snippets", vars: [{ name: "who", kind: "echo", params: [["echo", "alex"]] }] }),
    ] },
    "slack.yml": { name: "Slack", phrases: [
      p([";lgtm"], "Looks good", "LGTM :white_check_mark: Nice work!"),
      p([";brb"], "Be right back", "brb — stepping away for 10 minutes"),
    ] },
  };
  const apps = {
    "outlook.yml": [{ exe: ["OUTLOOK.EXE"], name: "Outlook" }],
    "code.yml": [{ exe: ["Code.exe"], name: "Visual Studio Code" }],
    "slack.yml": [{ exe: ["slack.exe"], name: "Slack" }],
  };
  const rehash = (f) => { files[f].hash = "h" + Math.random().toString(36).slice(2); files[f].phrases.forEach((x, i) => { x.index = i; x.line = 3 + i * 6; }); };
  for (const [f, d] of Object.entries(files)) { d.file = f; rehash(f); }

  const folderData = (f) => ({ name: files[f].name, file: f, phrases: files[f].phrases, warnings: [], hash: files[f].hash });
  const touch = (f) => { rehash(f); return folderData(f); };

  const scopeFor = (f) => {
    const a = apps[f];
    if (!a) return { kind: "all", apps: [] };
    return { kind: "only", apps: a.map((x) => ({ config: x.name.toLowerCase(), name: x.name, label: null, exe_names: x.exe, exec_pattern: "(?i)" + x.exe[0], exec_simple: true, title_pattern: null, title_text: null, class_pattern: null, os: null })) };
  };

  const yamlFor = (ph) => {
    const q = (s) => (/[:#{}\[\]&*!|>'"%@`,]|^\s|\s$|^$/.test(s) ? JSON.stringify(s) : s);
    const L = [`- trigger: ${q(ph.triggers[0])}`];
    if (ph.label) L.push(`  label: ${q(ph.label)}`);
    if (ph.kind === "html") L.push(`  html: ${JSON.stringify(ph.content)}`);
    else if (ph.content.includes("\n")) { L.push("  replace: |"); ph.content.split("\n").forEach((l) => L.push("    " + l)); }
    else L.push(`  replace: ${q(ph.content)}`);
    if (ph.vars.length) {
      L.push("  vars:");
      ph.vars.forEach((v) => { L.push(`    - name: ${v.name}`, `      type: ${v.kind}`); if (v.params.length) { L.push("      params:"); v.params.forEach(([k, val]) => L.push(`        ${k}: ${q(val)}`)); } });
    }
    return L;
  };

  const conflicts = () => {
    const all = [];
    Object.entries(files).forEach(([f, d]) => d.phrases.forEach((ph) => ph.triggers.forEach((t) => all.push({ f, ph, t }))));
    const out = [];
    for (const a of all) for (const b of all) {
      if (a === b || a.t === b.t || !b.t.startsWith(a.t)) continue;
      const mk = (x, role) => ({ file: x.f, index: x.ph.index, trigger: x.t, title: x.ph.label || x.t, word: "none", role });
      out.push({ kind: "prefix", certainty: "possible", low_risk: false, trigger: a.t, other_trigger: b.t, phrases: [mk(a, "shorter"), mk(b, "longer")] });
    }
    return out;
  };

  let kept = ["Email", "Dates", "Personal", "Support", "Snippets"];
  const state = { engineRunning: true };
  const find = (f, i) => files[f].phrases.find((x) => x.index === i);
  const apply = (ph, edit) => {
    if (edit.triggers) ph.triggers = edit.triggers;
    if (edit.label !== undefined) ph.label = edit.label;
    if (edit.body != null) { ph.content = edit.body; ph.preview = ph.kind === "html" ? strip(edit.body) : edit.body.replace(/\s+/g, " "); }
    if (edit.comment && edit.comment !== "Keep") ph.comment = edit.comment === "Clear" ? null : edit.comment.Set;
    if (edit.options) ph.options = { ...ph.options, ...edit.options };
  };
  const mkVars = (specs) => (specs || []).map((s) => ({ name: s.name, kind: s.type, params: s.params.map((q) => [q.key, q.value.t === "list" ? q.value.v.join("|") : q.value.v]) }));
  const add = (file, phrase, extra) => {
    const ph = p(phrase.triggers, phrase.label, phrase.body, { comment: phrase.comment, ...extra });
    ph.options = { ...NO_OPTS(), ...phrase.options };
    files[file].phrases.push(ph);
    return touch(file);
  };

  const handlers = {
    list_phrases: () => Object.keys(files).map(folderData),
    list_conflicts: conflicts,
    config_path: () => "C:\\Users\\alex\\AppData\\Roaming\\xpando\\config\\default.yml",
    folder_names_get: () => kept,
    folder_names_set: ({ names }) => { kept = names; },
    list_folder_info: () => Object.keys(files).map((f) => ({ file: f, name: files[f].name, scope: scopeFor(f), locked: null, scope_locked: null, configs: [], imported_by: [] })),
    engine_status: () => ({ kind: "xpando", path: "C:\\Users\\alex\\AppData\\Local\\xpando\\xpando-engine.exe", version: "2.4.1", bundled: true }),
    engine_run_state: () => ({ running: state.engineRunning, stopped_by_user: !state.engineRunning, at_login: true, problem: null }),
    search_shortcut_get: () => "ALT+SPACE",
    search_shortcut_set: ({ value }) => value,
    data_info: () => ({ path: "C:\\Users\\alex\\OneDrive\\xpando", is_default: false, default_path: "C:\\Users\\alex\\AppData\\Roaming\\xpando", status: { kind: "ok" }, problem: null, conflicts: [] }),
    check_data_dir: () => ({ kind: "ok" }),
    import_offer: () => null,
    update_state: () => ({ current: "2026.11.24", dev_build: false, beta: false, auto_check: true, last_check: Date.now() / 1000, skipped: null, note: null, can_install: true }),
    check_for_update: () => ({ status: "up_to_date" }),
    set_update_prefs: () => handlers.update_state(),
    app_icons: () => ({}),
    list_running_programs: () => [{ exe: "chrome.exe", name: "Google Chrome" }, { exe: "OUTLOOK.EXE", name: "Outlook" }, { exe: "slack.exe", name: "Slack" }, { exe: "Code.exe", name: "Visual Studio Code" }],
    list_installed_programs: () => [
      ["chrome.exe", "Google Chrome"], ["Code.exe", "Visual Studio Code"], ["EXCEL.EXE", "Excel"], ["firefox.exe", "Firefox"],
      ["notepad.exe", "Notepad"], ["OUTLOOK.EXE", "Outlook"], ["slack.exe", "Slack"], ["WINWORD.EXE", "Word"],
    ].map(([exe, name]) => ({ exe, name })),
    preview_phrase_where: ({ file, index, place }) => {
      const from = files[file];
      const names = place.names || [];
      return { from_file: file, from_name: from.name, to_file: null, to_name: names.join(", ") || "All apps", creates_folder: true, unchanged: false,
        notice: `“${find(file, index).label}” will work only in ${names.join(", ") || "all apps"}.`, warnings: [], conflicts: [], blocked: null,
        changes: [{ file: "word.yml", action: "create", detail: "New file for " + names.join(", "), backup: null }] };
    },
    set_phrase_where: ({ file, index, place }) => {
      const ph = find(file, index);
      const key = "word.yml";
      if (!files[key]) { files[key] = { name: "Word", file: key, phrases: [], hash: "" }; apps[key] = [{ exe: ["WINWORD.EXE"], name: "Word" }]; rehash(key); }
      files[file].phrases = files[file].phrases.filter((x) => x !== ph);
      files[key].phrases.push(ph);
      touch(file); touch(key);
      return { from_file: file, from_name: files[file].name, to_file: key, to_name: "Word", to_index: files[key].phrases.length - 1, created_folder: true, changes: [] };
    },
    get_entry_source: ({ file, index }) => { const ph = find(file, index); return ph ? { first_line: 3 + index * 6, lines: yamlFor(ph), valid: true, error: null } : null; },
    validate_entry_yaml: () => ({ ok: true, error: null, indent_shift: 0, notice: null }),
    get_phrase_vars: ({ file, index }) => (find(file, index)?.vars || []).map((v, i) => ({ name: v.name, type: v.kind, source: i, params: v.params.map(([key, val]) => ({ key, value: val.includes("|") ? { t: "list", v: val.split("|") } : { t: "text", v: val } })) })),
    sanitize_html: ({ html }) => ({ html, dropped: [] }),
    plain_to_html: ({ text }) => ({ html: "<p>" + text.replace(/\n\n/g, "</p><p>").replace(/\n/g, "<br>") + "</p>", lost: [] }),
    html_to_plain: ({ html }) => ({ text: strip(html), lost: [] }),
    save_phrase: ({ file, index, edit }) => { apply(find(file, index), edit); return touch(file); },
    save_rich_phrase: ({ file, index, edit, html }) => { const ph = find(file, index); apply(ph, { ...edit, body: null }); if (html != null) { ph.content = html; ph.preview = strip(html); ph.kind = "html"; } return touch(file); },
    save_phrase_vars: ({ file, index, change, edit }) => { const ph = find(file, index); apply(ph, edit); if (change.vars) ph.vars = mkVars(change.vars); if (change.body != null) { ph.content = change.body; ph.preview = change.body.replace(/\s+/g, " "); } return touch(file); },
    create_phrase: ({ file, phrase }) => add(file, phrase),
    create_phrase_with_vars: ({ file, phrase, variables }) => add(file, phrase, { vars: mkVars(variables) }),
    create_rich_phrase: ({ file, phrase, html }) => add(file, { ...phrase, body: html }, { kind: "html" }),
    delete_phrase: ({ file, index }) => { files[file].phrases = files[file].phrases.filter((x) => x.index !== index); return touch(file); },
  };

  const invoke = async (cmd, args) => {
    if (cmd.startsWith("plugin:")) return 1;
    const h = handlers[cmd];
    if (!h) { console.warn("[mock] unhandled command", cmd, JSON.stringify(args)); throw new Error("not available in the demo: " + cmd); }
    await new Promise((r) => setTimeout(r, 15));
    const out = h(args || {});
    return out === undefined ? null : JSON.parse(JSON.stringify(out));
  };
  window.__TAURI_INTERNALS__ = { invoke, transformCallback: () => 0, unregisterCallback() {}, convertFileSrc: (s) => s, metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } } };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__mock = { files, state, handlers };
})();
