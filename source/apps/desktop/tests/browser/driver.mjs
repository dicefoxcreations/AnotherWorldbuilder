/**
 * The test bench driver: remote-controlling Chrome instead of just taking
 * pictures.
 *
 * A screenshot shows what something looks like. The errors that count,
 * though, arise while *using* it — a half-typed character, a cursor in the
 * wrong place, a menu entry that inserts elsewhere. For that someone has to
 * really type.
 *
 * No Playwright, no dependency: Chrome ships a remote protocol, and Node has
 * WebSocket built in since version 22. Nothing more is needed.
 *
 * What is needed is a running Chrome with an open port:
 *
 * ```text
 * google-chrome-stable --headless=new --remote-debugging-port=9222 \
 *   --user-data-dir=/tmp/aw-chrome about:blank &
 * ```
 *
 * Then, say:
 *
 * ```text
 * node tests/browser/driver.mjs http://localhost:5321/?stage=blank picture.png
 * ```
 *
 * As a building block for scenarios of your own:
 *
 * ```js
 * const page = await Page.open("http://localhost:5321/");
 * await page.click(".template-text");
 * await page.type("<block title=\"Test\">");
 * await page.shot("after.png");
 * ```
 */

import { writeFileSync } from "node:fs";

const PORT = Number(process.env.AW_CHROME_PORT ?? 9222);

/**
 * Fetches the address of a page in the running Chrome — a new one if need be.
 *
 * Chrome binds sometimes to `127.0.0.1`, sometimes to `[::1]` — depending on
 * the run. Both are tried, otherwise one faces a “connection refused” on a
 * port that is actually open.
 */
async function target(url) {
  let last;
  for (const host of ["127.0.0.1", "[::1]"]) {
    const base = `http://${host}:${PORT}`;
    try {
      const fresh = await fetch(`${base}/json/new?${encodeURIComponent(url)}`, { method: "PUT" });
      if (fresh.ok) return (await fresh.json()).webSocketDebuggerUrl;
      const all = await (await fetch(`${base}/json`)).json();
      const page = all.find((t) => t.type === "page");
      if (page) return page.webSocketDebuggerUrl;
    } catch (e) {
      last = e;
    }
  }
  throw new Error(
    `Chrome does not answer on port ${PORT}. Is it running with --remote-debugging-port=${PORT}? ${last ?? ""}`,
  );
}

export class Page {
  #ws;
  #seq = 0;
  #pending = new Map();

  constructor(ws) {
    this.#ws = ws;
    ws.addEventListener("message", (e) => {
      const message = JSON.parse(e.data);
      const waiting = this.#pending.get(message.id);
      if (!waiting) return;
      this.#pending.delete(message.id);
      message.error ? waiting.reject(new Error(message.error.message)) : waiting.resolve(message.result);
    });
  }

  static async open(url, { width = 1400, height = 900 } = {}) {
    const ws = new WebSocket(await target(url));
    await new Promise((resolve, reject) => {
      ws.addEventListener("open", resolve, { once: true });
      ws.addEventListener("error", () => reject(new Error("Chrome does not answer.")), { once: true });
    });
    const page = new Page(ws);
    await page.call("Page.enable");
    await page.call("Runtime.enable");
    await page.call("Emulation.setDeviceMetricsOverride", {
      width: width,
      height: height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.call("Page.navigate", { url });
    await page.settle();
    return page;
  }

  call(method, params = {}) {
    const id = ++this.#seq;
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve, reject });
      this.#ws.send(JSON.stringify({ id, method, params }));
    });
  }

  /** Runs JavaScript in the page and returns the result. */
  async js(source) {
    const { result, exceptionDetails } = await this.call("Runtime.evaluate", {
      // Async, so `await` works in the snippet — to load one of the page's
      // modules, say.
      expression: `(async () => { ${source} })()`,
      returnByValue: true,
      awaitPromise: true,
    });
    if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? "error");
    return result.value;
  }

  /** Waits until the page has nothing more to do. */
  async settle(ms = 400) {
    await new Promise((r) => setTimeout(r, ms));
  }

  /** The screen centre of an element — for clicking. */
  async spot(selector) {
    const r = await this.js(`
      const el = document.querySelector(${JSON.stringify(selector)});
      if (!el) return null;
      const k = el.getBoundingClientRect();
      return { x: k.left + k.width / 2, y: k.top + k.height / 2 };
    `);
    if (!r) throw new Error(`Nothing found: ${selector}`);
    return r;
  }

  async click(selector, { right = false } = {}) {
    const { x, y } = await this.spot(selector);
    const button = right ? "right" : "left";
    for (const type of ["mousePressed", "mouseReleased"]) {
      await this.call("Input.dispatchMouseEvent", { type, x, y, button, clickCount: 1 });
    }
    await this.settle(120);
  }

  /** Clicks the first element whose text matches. */
  async clickText(selector, text) {
    const found = await this.js(`
      const el = [...document.querySelectorAll(${JSON.stringify(selector)})]
        .find((e) => e.textContent.trim().includes(${JSON.stringify(text)}));
      if (!el) return false;
      // In a menu that scrolls, the entry may lie below its edge.
      el.scrollIntoView({ block: "nearest" });
      el.setAttribute("data-driver", "target");
      return true;
    `);
    if (!found) throw new Error(`No “${text}” under ${selector}`);
    await this.click('[data-driver="target"]');
    await this.js(`document.querySelector('[data-driver]')?.removeAttribute("data-driver");`);
  }

  /**
   * Chooses something in a select.
   *
   * A click on a `<select>` only opens the list — the operating system draws
   * it, not the page. So the value is set and the event fired that a human
   * would have fired.
   */
  async choose(selector, value) {
    const ok = await this.js(`
      const el = document.querySelector(${JSON.stringify(selector)});
      if (!el) return false;
      const set = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value").set;
      set.call(el, ${JSON.stringify(value)});
      el.dispatchEvent(new Event("change", { bubbles: true }));
      return true;
    `);
    if (!ok) throw new Error(`No select found: ${selector}`);
    await this.settle(300);
  }

  /** Types — character by character, like a human. */
  async type(text) {
    for (const ch of text) {
      if (ch === "\n") {
        await this.key("Enter");
        continue;
      }
      await this.call("Input.dispatchKeyEvent", { type: "keyDown", text: ch });
      await this.call("Input.dispatchKeyEvent", { type: "keyUp", text: ch });
      await this.settle(12);
    }
    await this.settle(250);
  }

  async key(name) {
    const known = {
      Enter: { windowsVirtualKeyCode: 13, text: "\r", key: "Enter", code: "Enter" },
      Backspace: { windowsVirtualKeyCode: 8, key: "Backspace", code: "Backspace" },
      Escape: { windowsVirtualKeyCode: 27, key: "Escape", code: "Escape" },
      End: { windowsVirtualKeyCode: 35, key: "End", code: "End" },
    }[name];
    if (!known) throw new Error(`Unknown key: ${name}`);
    // Keys that produce a character (Enter) need `keyDown` with `text` —
    // `rawKeyDown` triggers the key but types nothing. Without this a text
    // field swallowed every line break.
    const down = known.text ? "keyDown" : "rawKeyDown";
    for (const type of [down, "keyUp"]) {
      await this.call("Input.dispatchKeyEvent", { type, ...known });
    }
    await this.settle(60);
  }

  /** Puts the caret in a text field, after a line one looks for. */
  async caretAfter(selector, mark) {
    await this.click(selector);
    const ok = await this.js(`
      const f = document.querySelector(${JSON.stringify(selector)});
      const spot = f.value.indexOf(${JSON.stringify(mark)});
      if (spot < 0) return false;
      const end = spot + ${JSON.stringify(mark)}.length;
      f.setSelectionRange(end, end);
      return true;
    `);
    if (!ok) throw new Error(`“${mark}” is not in the field.`);
  }

  async text(selector) {
    return this.js(`
      const el = document.querySelector(${JSON.stringify(selector)});
      return el ? (el.value ?? el.textContent) : null;
    `);
  }

  async shot(file) {
    const { data } = await this.call("Page.captureScreenshot", { format: "png" });
    writeFileSync(file, Buffer.from(data, "base64"));
    return file;
  }

  close() {
    this.#ws.close();
  }
}

// Called directly: open the page, take a picture, done.
if (process.argv[1]?.endsWith("driver.mjs")) {
  const [url, file = "testbench.png"] = process.argv.slice(2);
  if (!url) {
    console.error("Usage: node tests/browser/driver.mjs <url> [file.png]");
    process.exit(1);
  }
  const page = await Page.open(url);
  await page.settle(900);
  console.log("saved:", await page.shot(file));
  page.close();
}
