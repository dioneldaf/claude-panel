// Drag interaction: the window itself is moved by the Rust side (which follows the
// cursor with a short lag); this module adds the physical feel on top.
//
// Everything is spring-driven so it can be interrupted at any point and keeps its
// momentum: grab squashes the panel, movement tilts it and swings the mascot like
// a pendulum, shaking makes the mascot dizzy, and the drop lands with a bounce.
// The animation frame loop only runs while something is moving.

import { ShakeDetector, Spring } from "./physics";

export interface DragCallbacks {
  /** Asks the backend to start / stop moving the window. */
  startWindowDrag(): void;
  endWindowDrag(): void;
  onDragStart(): void;
  onShake(): void;
  onDrop(wasDizzy: boolean): void;
  reducedMotion(): boolean;
}

/** Movement in pixels before a press becomes a drag (keeps clicks and double-clicks intact). */
const DRAG_THRESHOLD = 4;
const MAX_TILT = 8;
const MAX_SWING = 38;
const INTERACTIVE = "button, a, input, select, textarea, [data-no-drag]";

export function installDrag(
  surface: HTMLElement,
  wobble: HTMLElement,
  mascot: HTMLElement,
  callbacks: DragCallbacks,
): { dropFromBackend(): void } {
  // Panel: critically-damped-ish for tilt, bouncier for the squash.
  const tilt = new Spring(170, 14);
  const scaleX = new Spring(420, 15);
  const scaleY = new Spring(420, 15);
  // Mascot: loosely damped so it keeps swinging after the cursor stops.
  const swing = new Spring(60, 4.5);
  const bob = new Spring(140, 9);
  const springs = [tilt, scaleX, scaleY, swing, bob];
  scaleX.value = scaleX.target = 1;
  scaleY.value = scaleY.target = 1;

  const shake = new ShakeDetector();
  let pressed = false;
  let dragging = false;
  let dizzy = false;
  let pointerId = -1;
  let startX = 0;
  let startY = 0;
  let lastX = 0;
  let lastY = 0;
  let lastMoveAt = 0;
  let vx = 0;
  let vy = 0;
  let frame = 0;
  let lastFrameAt = 0;

  const clamp = (v: number, limit: number) => Math.max(-limit, Math.min(limit, v));

  function loop(now: number): void {
    const dt = lastFrameAt ? (now - lastFrameAt) / 1000 : 1 / 60;
    lastFrameAt = now;

    // No pointer movement for a moment means the cursor is standing still.
    if (now - lastMoveAt > 60) {
      vx = 0;
      vy = 0;
    }
    if (dragging) {
      // The bottom trails behind the grab point; the mascot swings the other way.
      tilt.target = clamp(vx * 7, MAX_TILT);
      swing.target = clamp(-vx * 30, MAX_SWING);
      bob.target = clamp(-vy * 5, 5);
    }
    for (const spring of springs) spring.step(dt);

    wobble.style.transform = `rotate(${tilt.value.toFixed(2)}deg) scale(${scaleX.value.toFixed(3)}, ${scaleY.value.toFixed(3)})`;
    mascot.style.transform = `translateY(${bob.value.toFixed(1)}px) rotate(${swing.value.toFixed(1)}deg)`;

    if (!pressed && springs.every((s) => s.atRest(0.02))) {
      // Fully settled: clear inline transforms and stop scheduling frames.
      for (const spring of springs) spring.settle();
      wobble.style.transform = "";
      mascot.style.transform = "";
      frame = 0;
      lastFrameAt = 0;
      return;
    }
    frame = requestAnimationFrame(loop);
  }

  function wake(): void {
    if (callbacks.reducedMotion()) return;
    if (!frame) frame = requestAnimationFrame(loop);
  }

  function setTargets(grabbed: boolean): void {
    // Squash on grab: wider and flatter, like something soft being pinched.
    scaleX.target = grabbed ? 1.035 : 1;
    scaleY.target = grabbed ? 0.94 : 1;
    if (!grabbed) {
      tilt.target = 0;
      swing.target = 0;
      bob.target = 0;
    }
  }

  function release(): void {
    if (!pressed) return;
    pressed = false;
    const wasDragging = dragging;
    dragging = false;
    document.documentElement.removeAttribute("data-dragging");
    if (pointerId >= 0 && surface.hasPointerCapture(pointerId)) surface.releasePointerCapture(pointerId);
    pointerId = -1;
    setTargets(false);
    if (wasDragging) {
      callbacks.endWindowDrag();
      // Landing: an impulse squashes the panel and the spring bounces it back.
      scaleY.impulse(-2.4);
      scaleX.impulse(1.5);
      callbacks.onDrop(dizzy);
    }
    dizzy = false;
    shake.reset();
    wake();
  }

  surface.addEventListener("pointerdown", (event) => {
    // One pointer at a time; extra buttons or touches are ignored.
    if (pressed || event.button !== 0) return;
    if ((event.target as Element).closest(INTERACTIVE)) return;
    pressed = true;
    pointerId = event.pointerId;
    startX = lastX = event.screenX;
    startY = lastY = event.screenY;
    lastMoveAt = performance.now();
    vx = vy = 0;
    setTargets(true);
    wake();
  });

  surface.addEventListener("pointermove", (event) => {
    if (!pressed || event.pointerId !== pointerId) return;
    const now = performance.now();
    if (!dragging) {
      if (Math.hypot(event.screenX - startX, event.screenY - startY) < DRAG_THRESHOLD) return;
      dragging = true;
      // Capture so the drag survives the cursor outrunning the lagging window.
      surface.setPointerCapture(pointerId);
      document.documentElement.setAttribute("data-dragging", "");
      callbacks.startWindowDrag();
      callbacks.onDragStart();
    }
    const dt = Math.max(1, now - lastMoveAt);
    // Smoothed velocity in pixels per millisecond.
    vx = vx * 0.6 + ((event.screenX - lastX) / dt) * 0.4;
    vy = vy * 0.6 + ((event.screenY - lastY) / dt) * 0.4;
    lastX = event.screenX;
    lastY = event.screenY;
    lastMoveAt = now;
    if (!dizzy && shake.push(vx, now)) {
      dizzy = true;
      callbacks.onShake();
    }
    wake();
  });

  surface.addEventListener("pointerup", release);
  surface.addEventListener("pointercancel", release);
  window.addEventListener("blur", release);

  return {
    /** The backend saw the mouse button go up (authoritative end of the drag). */
    dropFromBackend: release,
  };
}

/** Pixel dust kicked up at the bottom corners of `panel` when it lands. */
export function dustPuff(layer: HTMLElement, panel: HTMLElement): void {
  const host = layer.getBoundingClientRect();
  const box = panel.getBoundingClientRect();
  const baseY = box.bottom - host.top - 4;
  const corners: Array<[number, number]> = [
    [box.left - host.left + 6, -1],
    [box.right - host.left - 10, 1],
  ];
  for (const [x, direction] of corners) {
    for (let i = 0; i < 3; i++) {
      const dot = document.createElement("div");
      dot.className = "dust";
      dot.style.left = `${x}px`;
      dot.style.top = `${baseY}px`;
      layer.appendChild(dot);
      const dx = direction * (6 + i * 5);
      const dy = -(2 + i * 3);
      dot
        .animate(
          [
            { transform: "translate(0, 0)", opacity: 0.9 },
            { transform: `translate(${dx}px, ${dy}px)`, opacity: 0 },
          ],
          // Starts fast and stops in five visible frames.
          { duration: 320 + i * 40, easing: "steps(5, jump-none)", fill: "forwards" },
        )
        .finished.then(() => dot.remove())
        .catch(() => dot.remove());
    }
  }
}
