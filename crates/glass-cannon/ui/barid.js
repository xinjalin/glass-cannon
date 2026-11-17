// Which action bar this page is (actionbar.html?bar=N, 1 to 6): its widget's name, for the
// overlay (bar 1 is "action-bar", the others "action-bar-N"). Loaded before panel.js, which
// tells the overlay its size under that name.
const BAR = Math.min(6, Math.max(1, Math.round(Number(new URLSearchParams(location.search).get("bar"))) || 1));
document.body.dataset.panel = BAR === 1 ? "action-bar" : `action-bar-${BAR}`;
