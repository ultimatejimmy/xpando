/*
 * xpando website. Vanilla JS, no dependencies, nothing is stored.
 *
 * 1. Detects the visitor's operating system and points the main Download buttons at the right installer.
 * 2. Looks up the latest release from the GitHub API (one anonymous request, sent without cookies or
 *    referrer) to link the exact files and show the version, date and sizes.
 * 3. Reveals sections as they scroll into view (CSS only applies the hidden state when motion is allowed).
 *
 * Without JavaScript, or when GitHub cannot be reached (offline, rate limit), every button simply links to
 * the latest release page, which always works.
 */
(function () {
  "use strict";

  var doc = document;
  var root = doc.documentElement;
  root.classList.add("js");

  var REPO = "ultimatejimmy/xpando";
  var API = "https://api.github.com/repos/" + REPO + "/releases/latest";
  var ASSET_PREFIX = "https://github.com/" + REPO + "/releases/download/";

  // Which release file belongs to which button.
  var ASSET_MATCHERS = {
    windows: /_x64-setup\.exe$/i,
    macos: /_universal\.dmg$/i,
    deb: /_amd64\.deb$/i,
    appimage: /_amd64\.AppImage$/i,
    sums: /^SHA256SUMS\.txt$/i
  };

  /** "windows", "macos", "linux", "mobile" (phones and tablets) or "other". */
  function detectOS() {
    var nav = window.navigator || {};
    var ua = nav.userAgent || "";
    var uaData = nav.userAgentData;
    var platform = (uaData && uaData.platform) || nav.platform || "";
    var all = platform + " " + ua;

    if (/android|iphone|ipad|ipod/i.test(all)) return "mobile";
    // iPadOS pretends to be a Mac but has a touch screen.
    if (/mac/i.test(platform) && nav.maxTouchPoints > 1) return "mobile";
    if (/cros/i.test(ua)) return "other";
    if (/win/i.test(platform) || /windows/i.test(ua)) return "windows";
    if (/mac/i.test(platform) || /mac os x/i.test(ua)) return "macos";
    if (/linux|x11|bsd/i.test(all)) return "linux";
    return "other";
  }

  var os = detectOS();

  // What the primary button says for each system, before and after the release is known.
  var PRIMARY = {
    windows: { key: "windows", label: "Download for Windows", sub: "64-bit installer" },
    macos: { key: "macos", label: "Download for macOS", sub: "Apple silicon and Intel" },
    linux: { key: "appimage", label: "Download for Linux", sub: "AppImage, .deb also available" },
    mobile: { key: null, label: "Get xpando for your computer", sub: "xpando runs on Windows, macOS and Linux" },
    other: { key: null, label: "Download xpando", sub: "Windows, macOS and Linux" }
  };
  var primary = PRIMARY[os] || PRIMARY.other;

  function all(selector) {
    return Array.prototype.slice.call(doc.querySelectorAll(selector));
  }

  function setText(el, text) {
    if (el) el.textContent = text;
  }

  function formatSize(bytes) {
    if (!bytes || bytes < 1) return "";
    var mb = bytes / (1024 * 1024);
    return mb >= 10 ? Math.round(mb) + " MB" : mb.toFixed(1) + " MB";
  }

  // Mark the visitor's own platform in the download section.
  function markYourSystem() {
    var wanted = { windows: ["windows"], macos: ["macos"], linux: ["linux-appimage", "linux-deb"] }[os];
    if (!wanted) return;
    all(".dl-card").forEach(function (card) {
      if (wanted.indexOf(card.getAttribute("data-os")) === -1) return;
      card.classList.add("is-yours");
      var tag = card.querySelector(".tag");
      if (tag) tag.hidden = false;
    });
  }

  // Immediate: the label for this system, with links still going to the release page.
  function applyPrimaryLabel(version) {
    all('[data-dl="primary"]').forEach(function (a) {
      setText(a.querySelector("[data-dl-label]"), primary.label);
      var sub = primary.sub;
      if (version && primary.key) sub = "Version " + version + " · " + sub;
      setText(a.querySelector("[data-dl-sub]"), sub);
      if (!primary.key) a.setAttribute("href", "#download");
    });
    all('[data-dl="primary-simple"]').forEach(function (a) {
      if (!primary.key) a.setAttribute("href", "#download");
    });
  }

  function applyRelease(release) {
    var version = String(release.tag_name || "").replace(/^v/i, "");
    var assets = {};
    (release.assets || []).forEach(function (asset) {
      var url = asset.browser_download_url || "";
      if (url.indexOf(ASSET_PREFIX) !== 0) return; // only ever link to this project's releases
      Object.keys(ASSET_MATCHERS).forEach(function (key) {
        if (!assets[key] && ASSET_MATCHERS[key].test(asset.name || "")) {
          assets[key] = { url: url, name: asset.name, size: asset.size };
        }
      });
    });

    // Every button for a file we found now links to the file itself.
    all("[data-dl]").forEach(function (a) {
      var key = a.getAttribute("data-dl");
      if (key === "primary" || key === "primary-simple") key = primary.key;
      if (key && assets[key]) a.setAttribute("href", assets[key].url);
    });

    // File names and sizes on the download cards.
    all(".dl-card").forEach(function (card) {
      var btn = card.querySelector("[data-dl]");
      var asset = btn && assets[btn.getAttribute("data-dl")];
      var line = card.querySelector("[data-file]");
      if (asset && line) {
        setText(line, asset.name);
        var size = formatSize(asset.size);
        if (size) {
          var span = doc.createElement("span");
          span.className = "size";
          span.textContent = size;
          line.appendChild(span);
        }
      }
    });

    if (version) {
      all("[data-version]").forEach(function (el) {
        setText(el, version);
      });
      applyPrimaryLabel(version);
    }

    if (release.published_at) {
      var when = new Date(release.published_at);
      if (!isNaN(when.getTime())) {
        var text = when.toLocaleDateString(undefined, { year: "numeric", month: "long", day: "numeric" });
        all("[data-released-date]").forEach(function (el) {
          setText(el, text);
        });
        all("[data-released]").forEach(function (el) {
          el.hidden = false;
        });
      }
    }
  }

  function fetchRelease() {
    if (!window.fetch) return;
    var controller = window.AbortController ? new AbortController() : null;
    var timer = controller ? setTimeout(function () { controller.abort(); }, 8000) : null;
    var options = { credentials: "omit", referrerPolicy: "no-referrer", cache: "no-cache" };
    if (controller) options.signal = controller.signal;
    window
      .fetch(API, options)
      .then(function (res) {
        if (!res.ok) throw new Error("GitHub answered " + res.status);
        return res.json();
      })
      .then(applyRelease)
      .catch(function () {
        /* Offline or rate limited: the buttons keep linking to the latest release page. */
      })
      .then(function () {
        if (timer) clearTimeout(timer);
      });
  }

  // Scroll reveal. The CSS only hides .reveal elements when this class is set and motion is allowed.
  function setupReveal() {
    var items = all(".reveal");
    if (!items.length) return;
    var reduce = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduce || !("IntersectionObserver" in window)) {
      items.forEach(function (el) { el.classList.add("in"); });
      return;
    }
    var io = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          if (entry.isIntersecting) {
            entry.target.classList.add("in");
            io.unobserve(entry.target);
          }
        });
      },
      { rootMargin: "0px 0px -8% 0px", threshold: 0.05 }
    );
    items.forEach(function (el) { io.observe(el); });
  }


  /* Click or press Enter on a screenshot to see it large, in a modal dialog. */
  function setupLightbox() {
    var imgs = doc.querySelectorAll(".frame img, .shot img");
    if (!imgs.length || typeof HTMLDialogElement === "undefined") return;

    var dlg = doc.createElement("dialog");
    dlg.className = "lightbox";
    dlg.setAttribute("aria-label", "Enlarged screenshot");
    dlg.innerHTML =
      '<button type="button" class="lb-close" aria-label="Close"><svg class="i" aria-hidden="true" focusable="false" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg></button>' +
      '<img alt="">' +
      '<p class="lb-caption"></p>';
    doc.body.appendChild(dlg);
    var big = dlg.querySelector("img");
    var cap = dlg.querySelector(".lb-caption");
    var opener = null;

    function open(img) {
      opener = img;
      big.src = img.currentSrc || img.src;
      big.alt = img.alt;
      big.style.setProperty("--ar", String((img.naturalWidth || img.width) / (img.naturalHeight || img.height)));
      cap.textContent = img.alt;
      dlg.showModal();
    }

    Array.prototype.forEach.call(imgs, function (img) {
      img.classList.add("zoomable");
      img.tabIndex = 0;
      img.setAttribute("role", "button");
      img.setAttribute("aria-haspopup", "dialog");
      var hint = doc.createElement("span");
      hint.className = "zoom-hint";
      hint.setAttribute("aria-hidden", "true");
      hint.innerHTML = '<svg class="i" focusable="false" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 3h6v6"/><path d="M9 21H3v-6"/><path d="m21 3-7 7"/><path d="m3 21 7-7"/></svg><span>Enlarge</span>';
      img.closest(".frame, .shot").appendChild(hint);
      img.addEventListener("click", function () { open(img); });
      img.addEventListener("keydown", function (e) {
        if (e.key === "Enter" || e.key === " ") { e.preventDefault(); open(img); }
      });
    });

    dlg.addEventListener("click", function (e) {
      // a click on the dark backdrop or the close button closes it; a click on the picture does not
      if (e.target === dlg || e.target.closest(".lb-close")) dlg.close();
    });
    dlg.addEventListener("close", function () {
      big.removeAttribute("src");
      if (opener) opener.focus();
    });
  }

  function start() {
    applyPrimaryLabel(null);
    markYourSystem();
    setupReveal();
    setupLightbox();
    fetchRelease();
  }

  if (doc.readyState === "loading") {
    doc.addEventListener("DOMContentLoaded", start);
  } else {
    start();
  }
})();
