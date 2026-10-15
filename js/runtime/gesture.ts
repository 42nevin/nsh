// `nsh/gesture`: pan gestures on the responder system.

import type {
  GestureResponderEvent,
  PanResponderCallbacks,
  PanResponderGestureState,
  PanResponderInstance,
  TouchPoint,
} from "./types";

let panStates = 0;

/** Pan gestures on top of the responder system (React Native's
 * `PanResponder`). */
export const PanResponder = {
  create(config: PanResponderCallbacks): PanResponderInstance {
    const gs: PanResponderGestureState = {
      stateID: ++panStates,
      moveX: 0,
      moveY: 0,
      x0: 0,
      y0: 0,
      dx: 0,
      dy: 0,
      vx: 0,
      vy: 0,
      numberActiveTouches: 0,
    };
    // The centroid and time of the last update, and the touches it had.
    let lastX = 0;
    let lastY = 0;
    let lastTime = 0;
    let lastCount = 0;
    const centroid = (touches: TouchPoint[]) => {
      let x = 0;
      let y = 0;
      for (const t of touches) {
        x += t.pageX;
        y += t.pageY;
      }
      const n = Math.max(1, touches.length);
      return [x / n, y / n];
    };
    const reset = (e: GestureResponderEvent) => {
      const [x, y] = centroid(e.touches);
      Object.assign(gs, { moveX: x, moveY: y, x0: x, y0: y, dx: 0, dy: 0, vx: 0, vy: 0 });
      gs.numberActiveTouches = e.touches.length;
      lastX = x;
      lastY = y;
      lastTime = e.timestamp;
      lastCount = e.touches.length;
    };
    const update = (e: GestureResponderEvent) => {
      if (e.timestamp === lastTime && e.touches.length === lastCount) return;
      const [x, y] = centroid(e.touches);
      gs.numberActiveTouches = e.touches.length;
      // A finger went down or up: the centroid jumps, which is no movement.
      if (e.touches.length === lastCount) {
        const dt = Math.max(1, e.timestamp - lastTime);
        gs.dx += x - lastX;
        gs.dy += y - lastY;
        // Smoothed over a few samples.
        gs.vx = gs.vx * 0.3 + ((x - lastX) / dt) * 0.7;
        gs.vy = gs.vy * 0.3 + ((y - lastY) / dt) * 0.7;
      }
      gs.moveX = x;
      gs.moveY = y;
      lastX = x;
      lastY = y;
      lastTime = e.timestamp;
      lastCount = e.touches.length;
    };
    return {
      panHandlers: {
        onStartShouldSetResponderCapture: (e) => {
          if (e.touches.length === 1) reset(e);
          gs.numberActiveTouches = e.touches.length;
          return config.onStartShouldSetPanResponderCapture?.(e, gs) ?? false;
        },
        onStartShouldSetResponder: (e) => config.onStartShouldSetPanResponder?.(e, gs) ?? false,
        onMoveShouldSetResponderCapture: (e) => {
          update(e);
          return config.onMoveShouldSetPanResponderCapture?.(e, gs) ?? false;
        },
        onMoveShouldSetResponder: (e) => {
          update(e);
          return config.onMoveShouldSetPanResponder?.(e, gs) ?? false;
        },
        onResponderGrant: (e) => {
          gs.x0 = gs.moveX = lastX = centroid(e.touches)[0];
          gs.y0 = gs.moveY = lastY = centroid(e.touches)[1];
          gs.dx = gs.dy = gs.vx = gs.vy = 0;
          lastTime = e.timestamp;
          lastCount = e.touches.length;
          config.onPanResponderGrant?.(e, gs);
          return config.onShouldBlockNativeResponder?.(e, gs) ?? true;
        },
        onResponderReject: (e) => config.onPanResponderReject?.(e, gs),
        onResponderStart: (e) => {
          update(e);
          config.onPanResponderStart?.(e, gs);
        },
        onResponderMove: (e) => {
          update(e);
          config.onPanResponderMove?.(e, gs);
        },
        onResponderEnd: (e) => {
          update(e);
          config.onPanResponderEnd?.(e, gs);
        },
        onResponderRelease: (e) => {
          config.onPanResponderRelease?.(e, gs);
          gs.stateID = ++panStates;
        },
        onResponderTerminate: (e) => {
          config.onPanResponderTerminate?.(e, gs);
          gs.stateID = ++panStates;
        },
        onResponderTerminationRequest: (e) => config.onPanResponderTerminationRequest?.(e, gs) ?? true,
      },
    };
  },
};
