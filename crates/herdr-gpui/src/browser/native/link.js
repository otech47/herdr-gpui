// Herdr GPUI installs this in every page's main frame. A click on a link to
// another site asks the app to open it in the system browser, and resting on
// one offers to copy or open it.
(function () {
  "use strict";
  if (window.__herdrLinks) return;
  Object.defineProperty(window, "__herdrLinks", { value: true });

  var DWELL_MS = 350;
  var GRACE_MS = 280;
  var COPIED_MS = 1200;
  var EDGE = 8;

  var post = function (action, url) {
    try { window.ipc.postMessage(JSON.stringify({ kind: "link", action: action, url: url })); } catch (_) {}
  };

  // A rendered markdown document stands for the address it was read from.
  var site = location.origin;
  var preview = location.protocol === "herdr-preview:" || location.hostname === "herdr-preview.localhost";
  if (preview && location.pathname === "/_markdown") {
    try { site = new URL(new URLSearchParams(location.search).get("source")).origin; } catch (_) {}
  }

  var linkOf = function (event) {
    var path = event.composedPath ? event.composedPath() : [event.target];
    for (var i = 0; i < path.length; i++) {
      var node = path[i];
      if (!node || node.nodeType !== Node.ELEMENT_NODE) continue;
      if ((node.localName === "a" || node.localName === "area") && node.hasAttribute("href")) return node;
    }
    return null;
  };

  var outside = function (link) {
    if (!link || link.hasAttribute("download")) return null;
    // An SVG link's href is an object.
    var raw = typeof link.href === "string" ? link.href : link.href && link.href.baseVal;
    var url;
    try { url = new URL(raw, document.baseURI); } catch (_) { return null; }
    if (url.protocol !== "http:" && url.protocol !== "https:") return null;
    return url.origin === site ? null : url.href;
  };

  var host = null;
  var bubble = null;
  var label = null;
  var copy = null;
  var shown = null;
  var pending = null;
  var point = { x: 0, y: 0 };
  var showTimer = 0;
  var hideTimer = 0;
  var copiedTimer = 0;

  var CSS =
    ".bubble{display:flex;align-items:center;gap:2px;box-sizing:border-box;max-width:min(460px,calc(100vw - 16px));padding:3px;border-radius:8px;background:#232327;color:#f4f4f5;border:1px solid rgba(255,255,255,.16);box-shadow:0 6px 20px rgba(0,0,0,.35);font:12px/16px -apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;-webkit-user-select:none;user-select:none}" +
    ".url{flex:1 1 auto;min-width:0;padding:3px 6px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:#a1a1aa}" +
    "button{all:unset;flex:none;padding:3px 8px;border-radius:5px;white-space:nowrap}" +
    "button:hover{background:rgba(255,255,255,.12)}" +
    "button:active{background:rgba(255,255,255,.22)}" +
    "@keyframes herdr-link-in{from{opacity:0;transform:translateY(-2px)}to{opacity:1;transform:none}}" +
    "@media (prefers-reduced-motion:no-preference){.bubble{animation:herdr-link-in .12s ease-out}}";

  var button = function (text) {
    var element = document.createElement("button");
    element.type = "button";
    element.tabIndex = -1;
    element.textContent = text;
    return element;
  };

  var build = function () {
    host = document.createElement("div");
    host.style.cssText = "all:initial;position:fixed;z-index:2147483647;left:0;top:0;display:none";
    var root = host.attachShadow({ mode: "closed" });
    // A constructed sheet applies under a policy that refuses inline styles.
    try {
      var sheet = new CSSStyleSheet();
      sheet.replaceSync(CSS);
      root.adoptedStyleSheets = [sheet];
    } catch (_) {
      var style = document.createElement("style");
      style.textContent = CSS;
      root.appendChild(style);
    }
    bubble = document.createElement("div");
    bubble.className = "bubble";
    label = document.createElement("span");
    label.className = "url";
    copy = button("Copy link");
    var open = button("Open in browser");
    bubble.appendChild(label);
    bubble.appendChild(copy);
    bubble.appendChild(open);
    root.appendChild(bubble);

    copy.addEventListener("click", function () {
      if (!shown) return;
      post("copy", shown.url);
      copy.textContent = "Copied";
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(function () { copy.textContent = "Copy link"; }, COPIED_MS);
    });
    open.addEventListener("click", function () {
      if (!shown) return;
      post("open", shown.url);
      hide();
    });
    // The window draws the cursor from the host's own style.
    bubble.addEventListener("mouseover", function (event) {
      host.style.cursor = event.target.localName === "button" ? "pointer" : "default";
    });
    // The page keeps its focus and selection.
    host.addEventListener("mousedown", function (event) { event.preventDefault(); });
    ["mousedown", "mouseup", "click", "dblclick", "contextmenu", "pointerdown", "pointerup"].forEach(function (type) {
      host.addEventListener(type, function (event) { event.stopPropagation(); });
    });
  };

  var hide = function () {
    clearTimeout(hideTimer);
    if (!shown) return;
    shown = null;
    host.style.display = "none";
  };

  var forget = function () {
    clearTimeout(showTimer);
    pending = null;
  };

  var dismiss = function () {
    forget();
    hide();
  };

  var hideSoon = function () {
    clearTimeout(hideTimer);
    hideTimer = setTimeout(hide, GRACE_MS);
  };

  // A link wrapped over several lines has one box per line.
  var lineOf = function (link) {
    var rects = link.getClientRects();
    for (var i = 0; i < rects.length; i++) {
      var rect = rects[i];
      if (point.y >= rect.top - 1 && point.y <= rect.bottom + 1 && point.x >= rect.left - 1 && point.x <= rect.right + 1) return rect;
    }
    return link.getBoundingClientRect();
  };

  var show = function (link, url) {
    if (window.__herdrAnnotate || !link.isConnected) return;
    if (!host) build();
    if (!host.isConnected) document.documentElement.appendChild(host);
    clearTimeout(hideTimer);
    clearTimeout(copiedTimer);
    copy.textContent = "Copy link";
    label.textContent = url.replace(/^https?:\/\//, "");
    label.title = url;
    shown = { link: link, url: url };
    host.style.cursor = "default";
    // Measured at the corner, where the last position cannot squeeze it.
    host.style.left = "0px";
    host.style.top = "0px";
    host.style.display = "block";
    // Restart the entrance for a bubble that moves to another link.
    bubble.style.animation = "none";
    void bubble.offsetWidth;
    bubble.style.animation = "";

    var size = host.getBoundingClientRect();
    var width = document.documentElement.clientWidth || window.innerWidth;
    var height = window.innerHeight;
    var line = lineOf(link);
    // Under the line, and never far from the mouse on a tall link.
    var top = Math.min(line.bottom, point.y + 18) + 4;
    if (top + size.height > height - EDGE) top = Math.max(line.top, point.y - 18) - size.height - 4;
    var left = Math.min(point.x - 14, width - size.width - EDGE);
    host.style.left = Math.max(EDGE, left) + "px";
    host.style.top = Math.max(EDGE, top) + "px";
  };

  var passive = { capture: true, passive: true };
  window.addEventListener("mousemove", function (event) {
    point = { x: event.clientX, y: event.clientY };
  }, passive);

  window.addEventListener("mouseover", function (event) {
    point = { x: event.clientX, y: event.clientY };
    if (host && event.target === host) {
      clearTimeout(hideTimer);
      return;
    }
    var link = linkOf(event);
    var url = outside(link);
    if (shown && shown.link === link) {
      clearTimeout(hideTimer);
      return;
    }
    if (shown) hideSoon();
    if (!url) {
      forget();
      return;
    }
    if (pending === link) return;
    pending = link;
    clearTimeout(showTimer);
    showTimer = setTimeout(function () {
      pending = null;
      show(link, url);
    }, DWELL_MS);
  }, passive);

  window.addEventListener("mouseout", function (event) {
    if (event.relatedTarget) return;
    forget();
    if (shown) hideSoon();
  }, passive);

  window.addEventListener("scroll", dismiss, passive);
  window.addEventListener("resize", dismiss, passive);
  window.addEventListener("blur", dismiss, passive);
  window.addEventListener("pagehide", dismiss, passive);
  window.addEventListener("mousedown", function (event) {
    if (!host || event.target !== host) dismiss();
  }, passive);
  window.addEventListener("keydown", function (event) {
    if (event.key === "Escape") dismiss();
  }, passive);

  // Bubble phase, so a link the page handled itself is left to it.
  window.addEventListener("click", function (event) {
    if (event.defaultPrevented || event.button !== 0) return;
    var url = outside(linkOf(event));
    if (!url) return;
    event.preventDefault();
    dismiss();
    post("open", url);
  });
})();
