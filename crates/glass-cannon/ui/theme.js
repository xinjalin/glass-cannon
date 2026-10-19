// Applies the saved appearance (theme, font, tint, row colouring) to a window.
// Loaded by both the overlay and the settings page.

let appliedAppearance = "";

function applyAppearance(a) {
  if (!a) return;
  const key = JSON.stringify(a);
  if (key === appliedAppearance) return;
  appliedAppearance = key;
  const root = document.documentElement;
  root.dataset.theme = a.theme;
  root.dataset.highlight = a.highlight;
  root.dataset.backdrop = a.backdrop;
  root.dataset.bars = a.bar_color;
  root.classList.toggle("minimal", a.minimal);
  root.classList.toggle("info-bar", a.info_bar);
  root.style.setProperty("--fs", `${a.font_size}px`);
  // A chosen font goes first, with the built-in monospace behind it. The app only
  // accepts names without quotes or other CSS syntax; this checks again.
  const font = a.font && !/["'\\;{}<>]/.test(a.font) ? `"${a.font}", ` : "";
  root.style.setProperty("--mono", `${font}"Cascadia Mono", "Cascadia Code", Consolas, monospace`);
  root.style.setProperty("--alpha", (a.opacity / 100).toFixed(2));
}
