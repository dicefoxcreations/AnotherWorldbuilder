import { useEffect, useState } from "react";
import { Icon } from "../../shared/ui/Icons";
import { AreaNav } from "../../shared/ui/AreaNav";
import { StorageArea } from "./Storage";
import { type ArticleView, type IdentityInfo, type NetworkView, type Template, type WorldInfo } from "../../api/client";
import { GalleryArea } from "../gallery/Gallery";
import { ShortcutsArea } from "./ShortcutsArea";
import { CalendarArea } from "./CalendarArea";
import { AppearanceArea } from "./Appearance";
import { DiceArea, SavedRolls } from "./DiceArea";
import { AboutArea } from "./AboutArea";
import { UpdatesArea } from "./UpdatesArea";
import { useUpdates } from "../../shared/Updates";
import { BrowserArea } from "./BrowserArea";
import { TransferArea } from "./Transfer";
import { BackupArea } from "../worlds/Backups";
import { TrashArea } from "./TrashArea";
import { isMobile } from "../../shared/platform";
import { PlaceArea } from "../worlds/WorldPlace";
import { BasicsArea, WorldDice } from "./BasicsArea";
import { InfoArea } from "./InfoArea";
import { NetworkArea } from "./NetworkArea";
import { PlayersArea } from "./PlayersArea";
import { TypesArea } from "./TypesArea";

/** The areas, in three groups: the open world's, this device's, the app's. */
const AREAS = [
  { id: "world", icon: "world", name: "World", group: "world" },
  { id: "players", icon: "players", name: "Players", group: "world" },
  { id: "types", icon: "types", name: "Types", group: "world" },
  { id: "images", icon: "images", name: "Images", group: "world" },
  { id: "calendar", icon: "calendar", name: "Calendar", group: "world" },
  { id: "dice", icon: "dice", name: "Dice", group: "world" },
  { id: "transfer", icon: "transfer", name: "Transfer", group: "world" },
  { id: "trash", icon: "delete", name: "Trash", group: "world" },
  { id: "layout", icon: "appearance", name: "Appearance", group: "device" },
  { id: "keys", icon: "shortcuts", name: "Shortcuts", group: "device" },
  { id: "network", icon: "network", name: "Network", group: "device" },
  { id: "browser", icon: "browser", name: "Browser", group: "device" },
  { id: "storage", icon: "storage", name: "Storage", group: "device" },
  { id: "updates", icon: "updates", name: "Updates", group: "app" },
  { id: "about", icon: "about", name: "About", group: "app" },
] as const;

const GROUPS = { world: "This world", device: "This device", app: "App" } as const;

type AreaId = (typeof AREAS)[number]["id"];

export function Options({
  world,
  me,
  templates,
  articles,
  onChanged,
  onArticlesChanged,
  onWorldChanged,
  onOpen,
  onNetworkChanged,
  onStorageChanged,
  onLeave,
  onClose,
  onError,
}: {
  /** The open world — none on the main menu, then only the network. */
  world: WorldInfo | null;
  me: IdentityInfo;
  templates: Template[];
  articles: ArticleView[];
  onChanged: (t: Template[]) => void;
  onArticlesChanged: () => void;
  onWorldChanged: () => void;
  /** Opens a document — the options close for it. */
  onOpen: (id: string) => void;
  onNetworkChanged: (n: NetworkView) => void;
  onStorageChanged: () => void;
  /** Back to the main menu, to another world. */
  onLeave?: () => void;
  onClose: () => void;
  onError: (e: unknown) => void;
}) {
  // What waits for the GM comes first.
  const due = world ? world.requests + (world.isGm ? world.waiting : 0) : 0;
  const update = useUpdates();
  const [area, setArea] = useState<AreaId>(world ? (due > 0 ? "players" : "world") : "network");
  // In a world: transfer and trash are the GM's. Outside one: what belongs
  // to this device — the dice's look among it. A phone has no browser to
  // link and no keyboard to bind.
  const areas = (world
    ? AREAS.filter((b) => (b.id !== "transfer" && b.id !== "trash") || world.isGm)
    : AREAS.filter((b) => b.group !== "world" || b.id === "dice").filter((b) => b.id !== "keys"))
    .filter((b) => !isMobile || (b.id !== "browser" && b.id !== "keys"));
  const groupOf = (b: (typeof AREAS)[number]) => (world || b.group !== "world" ? GROUPS[b.group] : GROUPS.device);

  useEffect(() => {
    const key = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("keydown", key);
    return () => document.removeEventListener("keydown", key);
  }, [onClose]);

  return (
    <div className="options-backdrop" onClick={onClose}>
      <div className="options" onClick={(e) => e.stopPropagation()}>
        <AreaNav
          head={
            world ? (
              <>
                <span className="world-icon small">{world.icon}</span>
                <strong>{world.name}</strong>
              </>
            ) : (
              <strong>Settings</strong>
            )
          }
          areas={areas.map((b) => ({
            ...b,
            group: groupOf(b),
            badge: b.id === "players" ? due : b.id === "updates" && update.kind === "ready" ? 1 : 0,
          }))}
          on={area}
          onPick={setArea}
        >
          {onLeave && (
            <button className="ghost" onClick={onLeave}>
              <span className="nav-icon"><Icon name="switch-world" /></span>
              Switch world
            </button>
          )}
          <button className="close" onClick={onClose}>
            Close
          </button>
        </AreaNav>

        <div className="options-body">
          {area === "network" && <NetworkArea onChanged={onNetworkChanged} onError={onError} />}
          {area === "browser" && <BrowserArea onError={onError} />}
          {area === "storage" && <StorageArea onMoved={onStorageChanged} onError={onError} />}
          {area === "layout" && <AppearanceArea />}
          {area === "dice" && (
            <DiceArea
              world={world?.isGm ? <WorldDice world={world} onChanged={onWorldChanged} onError={onError} /> : undefined}
              rolls={world ? <SavedRolls world={world} onError={onError} /> : undefined}
            />
          )}
          {area === "updates" && <UpdatesArea />}
          {area === "about" && <AboutArea />}
          {world && area === "types" && (
            <TypesArea
              world={world}
              templates={templates}
              articles={articles}
              onChanged={onChanged}
              onArticlesChanged={onArticlesChanged}
              onOpen={onOpen}
              onError={onError}
            />
          )}
          {world && area === "players" && (
            <PlayersArea
              world={world}
              me={me}
              templates={templates}
              articles={articles}
              onChanged={onWorldChanged}
              onOpen={onOpen}
              onError={onError}
            />
          )}
          {world && area === "world" && (
            <>
              <InfoArea world={world} onChanged={onWorldChanged} onError={onError} />
              <PlaceArea world={world} onChanged={onWorldChanged} onError={onError} />
              <BackupArea world={world} onError={onError} />
              {world.isGm && <BasicsArea world={world} onChanged={onWorldChanged} onError={onError} />}
            </>
          )}
          {world && area === "images" && <GalleryArea world={world} onOpen={onOpen} onError={onError} />}
          {world && area === "trash" && <TrashArea world={world} templates={templates} onChanged={onArticlesChanged} onError={onError} />}
          {world && area === "keys" && <ShortcutsArea world={world} templates={templates} />}
          {world && area === "calendar" && <CalendarArea world={world} onChanged={onWorldChanged} onError={onError} />}
          {world && area === "transfer" && (
            <TransferArea
              world={world}
              templates={templates}
              articles={articles}
              onImported={() => {
                onArticlesChanged();
                onWorldChanged();
              }}
              onError={onError}
            />
          )}
        </div>
      </div>
    </div>
  );
}
