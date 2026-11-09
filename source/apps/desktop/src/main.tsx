import React from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import App from "./app/App";
import { Legend } from "./features/templates/TemplateLegend";
import { ServerWindow } from "./features/server/ServerWindow";
import { watchApartClicks } from "./shared/apart";
import { followKeptColors } from "./shared/Theme";
import "./shared/ui/dicefox-icons/DiceFoxIcons.css";
import "./styles/global.css";

// The same bundle carries every window. The legend window gets only the
// legend — no header bar, no world, no state; AW Server only its own window.
const only = new URLSearchParams(location.search).get("window");

// No browser menu on right-click — not in the app, nowhere. Where a
// right-click means something, the app shows its own menu.
document.addEventListener("contextmenu", (e) => e.preventDefault());

// The reader's colours, before anything is drawn.
followKeptColors();

// Ctrl+click and the middle button open a place in a window of its own.
watchApartClicks();

// What goes wrong in the page goes into the app's log.
const logged = (text: string) => invoke("app_log", { kind: "page", text }).catch(() => {});
window.addEventListener("error", (e) => void logged(`${e.message} (${e.filename}:${e.lineno})`));
window.addEventListener("unhandledrejection", (e) => void logged(`unhandled: ${String(e.reason)}`));

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {only === "legend" ? <Legend /> : only === "server" ? <ServerWindow /> : <App />}
  </React.StrictMode>,
);
