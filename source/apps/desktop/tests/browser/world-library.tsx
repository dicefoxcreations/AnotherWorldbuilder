/** UI-only fixture: the world library with a local root and a shared folder. */
import { useState } from "react";
import { createRoot } from "react-dom/client";
import { MainMenu } from "../../src/app/MainMenu";
import { StorageArea } from "../../src/features/settings/Storage";
import { api, type WorldInfo, type StorageView } from "../../src/api/client";
import "../../src/styles/global.css";

const local = "/data/worlds";
const second = "/mnt/My cloud drive/Worlds from another GM";
const storage: StorageView = { state: "ready", profile: "/data/profile", worlds: local, locations: [
  { path: local, primary: true, available: true }, { path: second, primary: false, available: true },
], together: null, suggested: "/data", problem: null };
const world = (id: string, name: string, root: string): WorldInfo => ({
  id, name, folder: `${root}/world-${id}`, icon: "🌍", publicKey: "public", tokenFile: "profile/owner.key",
  role: "gm", isGm: true, attributes: [], members: [], articleCount: 3, home: "home", requests: 0, invited: [], handover: null, moderated: false, waiting: 0, versionLimit: 50,
});
const worlds = [world("local", "Local campaign", local), world("mounted", "Shared campaign", second)];
api.storageStatus = async () => structuredClone(storage);
api.storageLocations = async paths => { storage.locations = [storage.locations[0], ...paths.map(path => ({ path, primary: false, available: true }))]; return structuredClone(storage); };
api.worldCreate = async (name, destination) => {
  const w = world(`created-${worlds.length}`, name, destination.path);
  worlds.push(w); return structuredClone(w);
};
function Fixture() {
  const [items, setItems] = useState(worlds);
  const [section, setSection] = useState("library");
  const [message, setMessage] = useState("");
  const changed = () => setItems([...worlds]);
  const onError = (e: unknown) => setMessage(`ERROR: ${String(e)}`);
  return <main style={{ padding: 32 }}><nav className="line"><button onClick={() => setSection("library")}>Library</button><button onClick={() => setSection("storage")}>Storage</button></nav>
    <h1>Worlds</h1><p role="status">{message}</p>
    {section === "library" ? <MainMenu worlds={items} network={{ enabled: false, internet: false, reachable: [], via: [], stun: [], running: false, worlds: [] }} onOpen={id => setMessage(`Opened ${id}`)} onChanged={changed} onSettings={() => setSection("storage")} onError={onError} /> :
    <StorageArea onMoved={changed} onError={onError} />}
  </main>;
}
createRoot(document.getElementById("root")!).render(<Fixture />);
