// Applies the saved appearance (theme, font, tint, row colouring) to a window.
// Loaded by the overlay, the analysis window and the settings page.

let appliedAppearance = "";

const isHexColor = (c) => /^#[0-9a-f]{6}$/i.test(c || "");

// A colour picked by hand as the theme: the accent itself, a lighter one for numbers, and a
// very dark shade of it for the tint, like the built-in themes.
function customTheme(root, hex) {
  const rgb = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
  const light = rgb.map((c) => Math.round(c + (255 - c) * 0.45));
  root.style.setProperty("--accent", hex);
  root.style.setProperty("--accent-2", `rgb(${light.join(", ")})`);
  root.style.setProperty("--tint-rgb", rgb.map((c) => Math.round(c * 0.09)).join(", "));
}

function applyAppearance(a) {
  if (!a) return;
  const key = JSON.stringify(a);
  if (key === appliedAppearance) return;
  appliedAppearance = key;
  const root = document.documentElement;
  root.dataset.theme = isHexColor(a.theme) ? "custom" : a.theme;
  for (const v of ["--accent", "--accent-2", "--tint-rgb"]) root.style.removeProperty(v);
  if (isHexColor(a.theme)) customTheme(root, a.theme);
  root.dataset.highlight = a.highlight;
  root.dataset.backdrop = a.backdrop;
  root.dataset.bars = isHexColor(a.bar_color) ? "custom" : a.bar_color;
  if (isHexColor(a.bar_color)) root.style.setProperty("--bar", a.bar_color);
  else root.style.removeProperty("--bar");
  root.classList.toggle("minimal", a.minimal);
  root.classList.toggle("info-bar", a.info_bar);
  root.style.setProperty("--fs", `${a.font_size}px`);
  // A chosen font goes first, with the built-in monospace behind it. The app only
  // accepts names without quotes or other CSS syntax; this checks again.
  const font = a.font && !/["'\\;{}<>]/.test(a.font) ? `"${a.font}", ` : "";
  root.style.setProperty("--mono", `${font}"Cascadia Mono", "Cascadia Code", Consolas, monospace`);
  root.style.setProperty("--alpha", (a.opacity / 100).toFixed(2));
}
