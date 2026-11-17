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
