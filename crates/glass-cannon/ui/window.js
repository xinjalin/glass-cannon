// The windows' lights (settings, history, analysis…), at the title bar's right: green keeps
// the window on top (they start on top), red closes (each page's own handler, on #close).
// No minimise: these have no taskbar button, so a minimised one was only a closed one
// that was harder to get back (the launcher, which has one, keeps its own).

(() => {
  const w = window.__TAURI__.window.getCurrentWindow();
  const pin = document.getElementById("pin");
  if (pin) {
    pin.addEventListener("click", async () => {
      const on = pin.getAttribute("aria-pressed") !== "true";
      await w.setAlwaysOnTop(on).catch(() => {});
      pin.setAttribute("aria-pressed", String(on));
      pin.title = on ? "Kept on top · click to let other windows cover it" : "Keep on top";
    });
  }
})();
