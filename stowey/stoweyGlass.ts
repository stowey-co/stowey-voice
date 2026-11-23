// Stowey Voice: tells the app where the pill is, so that macOS can put glass
// behind it. A web view cannot blur what lies behind its own window, so the
// glass is a native view under this one, and it has to follow the pill.
import { invoke } from "@tauri-apps/api/core";

type Frame = {
  x: number;
  y: number;
  width: number;
  height: number;
  radius: number;
  alpha: number;
};

const HIDDEN: Frame = { x: 0, y: 0, width: 0, height: 0, radius: 0, alpha: 0 };

let sent = "";
let until = 0;
let running = false;

function measure(): Frame {
  const card = document.querySelector<HTMLElement>(".scard");
  if (!card) return HIDDEN;
  const box = card.getBoundingClientRect();
  const style = getComputedStyle(card);
  const stage = card.closest<HTMLElement>(".ov-stage");
  const fade = stage ? Number(getComputedStyle(stage).opacity) : 1;
  const alpha = Number(style.opacity) * fade;
  // Upright on a side edge, the narrow side is the width.
  const radius = Math.min(
    parseFloat(style.borderTopLeftRadius) || 0,
    Math.min(box.width, box.height) / 2,
  );
  return {
    x: box.left,
    y: box.top,
    width: box.width,
    height: box.height,
    radius,
    alpha: Number.isFinite(alpha) ? alpha : 0,
  };
}

function report() {
  const frame = measure();
  const key = [
    frame.x,
    frame.y,
    frame.width,
    frame.height,
    frame.radius,
    frame.alpha,
  ]
    .map((value) => value.toFixed(2))
    .join(" ");
  if (key === sent) return;
  sent = key;
  // The answer is what the pill wears, dark or light: the opposite of the
  // Mac's mode. The stylesheet colours the bars to match.
  invoke<string>("stowey_glass", { frame })
    .then((tone) => {
      document.documentElement.dataset.pill = tone;
    })
    .catch(() => {});
}

// The pill fades and grows over a few hundred milliseconds. Follow it frame
// by frame for a little longer than that, then rest until something changes.
function tick() {
  report();
  if (performance.now() < until) requestAnimationFrame(tick);
  else running = false;
}

function watch() {
  until = performance.now() + 700;
  if (running) return;
  running = true;
  requestAnimationFrame(tick);
}

invoke<string>("stowey_glass_kind")
  .then((kind) => {
    document.documentElement.dataset.glass = kind;
  })
  .catch(() => {
    document.documentElement.dataset.glass = "none";
  })
  .finally(watch);

const root = document.getElementById("root");
if (root) {
  new MutationObserver(watch).observe(root, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ["class", "style"],
  });
}
window.addEventListener("resize", watch);
