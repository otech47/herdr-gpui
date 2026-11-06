// Herdr GPUI installs this in every page's main frame. The window draws the
// cursor over its native pages, so the page reports the cursor it wants
// under the mouse and the window shows that one.
(function () {
  "use strict";
  if (window.__herdrCursor) return;
  Object.defineProperty(window, "__herdrCursor", { value: true });

  // The first report always goes out: the window may still show the cursor
  // of the page this one replaced.
  var shown = null;
  var point = null;
  var post = function (cursor) {
    if (cursor === shown) return;
    shown = cursor;
    try { window.ipc.postMessage(JSON.stringify({ kind: "cursor", cursor: cursor })); } catch (_) {}
  };

  var buttons = /^(button|submit|reset|checkbox|radio|range|color|file|image|hidden)$/i;
  var editable = function (element) {
    if (element.isContentEditable || element.localName === "textarea") return true;
    return element.localName === "input" && !buttons.test(element.type);
  };

  var overText = function (element, x, y) {
    for (var node = element.firstChild; node; node = node.nextSibling) {
      if (node.nodeType !== Node.TEXT_NODE || !node.data.trim()) continue;
      var range = document.createRange();
      range.selectNodeContents(node);
      var rects = range.getClientRects();
      for (var i = 0; i < rects.length; i++) {
        var rect = rects[i];
        if (x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom) return true;
      }
    }
    return false;
  };

  // What WebKit would show: the CSS cursor, where `auto` means a hand on a
  // link and a text cursor over editable fields and selectable text.
  var wanted = function (element, x, y) {
    if (!element || element.nodeType !== Node.ELEMENT_NODE) return "default";
    var style = getComputedStyle(element);
    // An image cursor ends with the keyword to use when it cannot load.
    var cursor = style.cursor.split(",").pop().trim();
    if (cursor !== "auto") return cursor;
    if (element.closest("a[href], area[href]")) return "pointer";
    if (editable(element)) return "text";
    if (style.userSelect === "none" || style.webkitUserSelect === "none") return "default";
    return overText(element, x, y) ? "text" : "default";
  };

  var options = { capture: true, passive: true };
  window.addEventListener("mousemove", function (event) {
    point = { x: event.clientX, y: event.clientY };
    post(wanted(event.target, point.x, point.y));
  }, options);
  window.addEventListener("scroll", function () {
    if (point) post(wanted(document.elementFromPoint(point.x, point.y), point.x, point.y));
  }, options);
  window.addEventListener("mouseout", function (event) {
    if (event.relatedTarget) return;
    point = null;
    post("default");
  }, options);
})();
