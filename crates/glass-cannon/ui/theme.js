// Applies the saved appearance (theme, font size, tint, row colouring) to a window.
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
  root.style.setProperty("--alpha", (a.opacity / 100).toFixed(2));
}
