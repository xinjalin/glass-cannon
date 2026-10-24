// The windows' lights (settings, history, analysis), at the title bar's right: yellow
// minimises, green keeps the window on top (they start on top), red closes (each page's
// own handler, on #close).

(() => {
  const w = window.__TAURI__.window.getCurrentWindow();
  const min = document.getElementById("min");
  const pin = document.getElementById("pin");
  if (min) min.addEventListener("click", () => w.minimize());
  if (pin) {
    pin.addEventListener("click", async () => {
      const on = pin.getAttribute("aria-pressed") !== "true";
      await w.setAlwaysOnTop(on).catch(() => {});
      pin.setAttribute("aria-pressed", String(on));
      pin.title = on ? "Kept on top · click to let other windows cover it" : "Keep on top";
    });
  }
})();
