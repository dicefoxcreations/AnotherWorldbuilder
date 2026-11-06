import assert from "node:assert/strict";
import test from "node:test";
import { anchorDrawn, leadTo, pendingAnchor } from "../../src/shared/Anchors.ts";

// A small layout surface: resize and draw callbacks run only when the test
// advances them, so late content and rapid clicks are deterministic.
function surface(t) {
  const names = ["document", "window", "CSS", "ResizeObserver", "MutationObserver", "requestAnimationFrame", "cancelAnimationFrame"];
  const kept = new Map(names.map((name) => [name, Object.getOwnPropertyDescriptor(globalThis, name)]));
  const document = new EventTarget(),
    window = new EventTarget(),
    page = new EventTarget();
  page.parentElement = page;
  const frames = new Map(),
    resizes = [];
  let frame = 0,
    visible = true,
    y = 1000,
    scrollY = 0;
  const calls = [],
    parts = new Map();
  for (const id of ["quirks", "boons"]) {
    const part = {
      isConnected: true,
      classList: { add() {}, remove() {} },
      offsetWidth: 0,
      closest: (selector) => (selector === ".article-page" ? page : null),
      scrollIntoView: (options) => {
        calls.push({ id, options });
        scrollY = y;
      },
    };
    const spot = {
      parentElement: part,
      closest: (selector) => (selector === ".article-page" ? page : selector === "li, p, h1, h2, h3, h4, h5, h6" ? part : null),
    };
    parts.set(id, { part, spot });
  }
  document.querySelectorAll = (selector) => (visible ? [...parts].filter(([id]) => selector.includes(`"${id}"`)).map(([, p]) => p.spot) : []);
  Object.assign(globalThis, {
    document,
    window,
    CSS: { escape: (id) => id },
    requestAnimationFrame: (callback) => {
      frames.set(++frame, callback);
      return frame;
    },
    cancelAnimationFrame: (id) => frames.delete(id),
    ResizeObserver: class {
      constructor(callback) {
        this.callback = callback;
        this.active = false;
        resizes.push(this);
      }
      observe() {
        this.active = true;
      }
      disconnect() {
        this.active = false;
      }
    },
    MutationObserver: class {
      constructor(callback) {
        this.callback = callback;
      }
      observe() {}
      disconnect() {}
    },
  });
  t.after(() => {
    document.dispatchEvent(new Event("wheel"));
    for (const name of names) {
      const descriptor = kept.get(name);
      if (descriptor) Object.defineProperty(globalThis, name, descriptor);
      else delete globalThis[name];
    }
  });
  return {
    calls,
    show: () => {
      visible = true;
    },
    hide: () => {
      visible = false;
    },
    flush: () => {
      const ready = [...frames.values()];
      frames.clear();
      for (const callback of ready) callback();
    },
    move: () => {
      y += 650;
      for (const observer of resizes) if (observer.active) observer.callback([]);
    },
    get scrollY() {
      return scrollY;
    },
    get y() {
      return y;
    },
    wheel: () => document.dispatchEvent(new Event("wheel")),
  };
}

test("a heading jump follows late layout changes and releases on user scrolling", (t) => {
  const s = surface(t);
  leadTo("chapter", "quirks", "chapter", () => assert.fail("Already on this page"));
  assert.deepEqual(s.calls, [{ id: "quirks", options: { block: "start", behavior: "instant" } }]);
  s.move();
  s.flush();
  assert.equal(s.calls.length, 2);
  assert.equal(s.scrollY, s.y);
  s.wheel();
  s.move();
  s.flush();
  assert.equal(s.calls.length, 2, "Reading by hand must stop automatic alignment");
});

test("an anchor drawn in a preview does not consume a pending page destination", (t) => {
  const s = surface(t);
  s.hide();
  let opens = 0;
  leadTo("chapter", "quirks", null, () => opens++);
  assert.equal(opens, 1);
  anchorDrawn("chapter", "quirks");
  s.flush();
  assert.equal(pendingAnchor("chapter"), "quirks");
  assert.equal(s.calls.length, 0);
  s.show();
  anchorDrawn("chapter", "quirks");
  s.flush();
  assert.equal(pendingAnchor("chapter"), null);
  assert.equal(s.calls[0].id, "quirks");
});

test("a queued old destination cannot override a newer click", (t) => {
  const s = surface(t);
  leadTo("chapter", "quirks", null, () => {});
  anchorDrawn("chapter", "quirks");
  leadTo("chapter", "boons", "chapter", () => assert.fail("Already on this page"));
  s.flush();
  assert.deepEqual(
    s.calls.map((call) => call.id),
    ["boons"],
  );
  assert.equal(pendingAnchor("chapter"), null);
});
