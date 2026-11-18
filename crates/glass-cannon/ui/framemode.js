// The settings page in a widget's panel on the overlay (settings.html?frame=<widget>): the
// panel beside a widget while you arrange the overlay shows just that widget's settings
// (the sections marked data-for it). A frame doesn't get the app itself, so it borrows the
// overlay's; and it has no window of its own to size. Loaded first.
const FRAME = new URLSearchParams(location.search).get("frame");
if (FRAME) {
  if (!window.__TAURI__ && window.parent !== window) window.__TAURI__ = window.parent.__TAURI__;
  document.documentElement.classList.add("framed");
  document.addEventListener("DOMContentLoaded", () => {
    for (const box of document.querySelectorAll("[data-for]")) {
      if (box.dataset.for.split(" ").includes(FRAME)) box.classList.add("frame-show");
    }
  });
}

// The panel as tall as what it shows (up to the most the overlay gives it): its height,
// told to the overlay as it changes.
function tellPanelHeight(box) {
  let sent = 0;
  // The part that scrolls, and what's around it on the page (its padding, the message line
  // under it): the panel's as tall as both, so nothing's left to scroll.
  const scroller = box.closest("#scroll") || box;
  const tell = () => {
    const top = box.getBoundingClientRect().top;
    let bottom = top;
    for (const k of box.children) {
      if (!k.getClientRects().length) continue;
      bottom = Math.max(bottom, k.getBoundingClientRect().bottom + parseFloat(getComputedStyle(k).marginBottom || 0));
    }
    const around = innerHeight - scroller.clientHeight;
    const h = Math.ceil(bottom - top + around) + 2;
    if (Math.abs(h - sent) < 2) return;
    sent = h;
    window.parent.postMessage({ type: "gc-panel-height", height: h }, "*");
  };
  new ResizeObserver(tell).observe(box);
  for (const k of box.children) new ResizeObserver(tell).observe(k);
  new MutationObserver(tell).observe(box, { subtree: true, childList: true, attributes: true, attributeFilter: ["class", "hidden"] });
  tell();
}
if (FRAME) document.addEventListener("DOMContentLoaded", () => tellPanelHeight(document.getElementById("panels")));
