// Applies the saved appearance (theme, font, tint, row colouring) to a window.
// Loaded by the overlay, the analysis window and the settings page.

let appliedAppearance = "";

const isHexColor = (c) => /^#[0-9a-f]{6}$/i.test(c || "");

// Text in a picked colour on the dark glass is lightened (same hue) to at least this
// lightness, like the built-in colours. (style.rs does the same for the window's frame.)
// Fills keep the colour as picked, with text on them light or dark by contrast (inkOn).
const MIN_LIGHTNESS = 0.62;
const DARK_INK = "#1b0f11";
const LIGHT_INK = "#fff7f3";

// The text colour that reads best on a fill of `hex`: the one with more contrast (WCAG).
function inkOn(hex) {
  const lum = (h) => {
    const [r, g, b] = [1, 3, 5].map((i) => {
      const c = parseInt(h.slice(i, i + 2), 16) / 255;
      return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
    });
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  };
  const contrast = (a, b) => (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
  const l = lum(hex);
  return contrast(l, lum(LIGHT_INK)) > contrast(l, lum(DARK_INK)) ? LIGHT_INK : DARK_INK;
}

function readable(hex) {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (l >= MIN_LIGHTNESS) return hex;
  const d = max - min;
  const s = d === 0 ? 0 : d / (1 - Math.abs(2 * l - 1));
  let h = 0;
  // Kept in 0-6: between red and magenta (more blue than green) `%` alone goes negative,
  // which made a channel negative and the colour invalid (as style.rs's `rem_euclid` does).
  if (d) h = max === r ? ((((g - b) / d) % 6) + 6) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  // Back from hue, saturation and the raised lightness.
  const L = MIN_LIGHTNESS;
  const c = (1 - Math.abs(2 * L - 1)) * s;
  const x = c * (1 - Math.abs((h % 2) - 1));
  const m = L - c / 2;
  const [r1, g1, b1] = [[c, x, 0], [x, c, 0], [0, c, x], [0, x, c], [x, 0, c], [c, 0, x]][((Math.floor(h) % 6) + 6) % 6];
  return "#" + [r1, g1, b1].map((v) => Math.round((v + m) * 255).toString(16).padStart(2, "0")).join("");
}

// A colour picked by hand as the theme: the accent for text (made readable), a lighter one
// for numbers, a very dark shade of it for the tint, like the built-in themes; and the
// colour itself for fills, with text on them that reads.
function customTheme(root, picked) {
  const hex = readable(picked);
  const rgb = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
  const light = rgb.map((c) => Math.round(c + (255 - c) * 0.45));
  root.style.setProperty("--accent", hex);
  root.style.setProperty("--accent-fill", picked);
  root.style.setProperty("--ink", inkOn(picked));
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
  for (const v of ["--accent", "--accent-2", "--tint-rgb", "--accent-fill", "--ink"]) root.style.removeProperty(v);
  if (isHexColor(a.theme)) customTheme(root, a.theme);
  root.dataset.highlight = a.highlight;
  root.dataset.backdrop = a.backdrop;
  root.dataset.bars = isHexColor(a.bar_color) ? "custom" : a.bar_color;
  if (isHexColor(a.bar_color)) {
    root.style.setProperty("--bar", a.bar_color);
    root.style.setProperty("--bar-ink", inkOn(a.bar_color));
  } else {
    root.style.removeProperty("--bar");
    root.style.removeProperty("--bar-ink");
  }
  root.classList.toggle("minimal", a.minimal || root.classList.contains("widget"));
  root.classList.toggle("info-bar", a.info_bar);
  root.style.setProperty("--fs", `${a.font_size}px`);
  // A chosen font goes first, with the built-in monospace behind it. The app only
  // accepts names without quotes or other CSS syntax; this checks again.
  const font = a.font && !/["'\\;{}<>]/.test(a.font) ? `"${a.font}", ` : "";
  root.style.setProperty("--mono", `${font}"Cascadia Mono", "Cascadia Code", Consolas, monospace`);
  root.style.setProperty("--alpha", (a.opacity / 100).toFixed(2));
}
