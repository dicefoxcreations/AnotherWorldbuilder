import { useEffect, useRef, useState } from "react";
import { Icon } from "../../shared/ui/Icons";
import { open } from "@tauri-apps/plugin-dialog";
import { DeleteDialog, DeleteButton } from "../../shared/ui/Delete";
import { api, type ArticleView, type IdentityInfo, type JoinRequest, type MemberInfo, type WaitingVersion, type Template, type WorldInfo } from "../../api/client";
import { when } from "../wiki/Versions";
import { SearchPick, type Pickable } from "../../shared/ui/SearchPick";
import { typeAttribute } from "../../shared/ui/Visibility";

/** The attributes every member has anyway — not worth a mention. */
const PLAIN = ["gm", "players", "all"];


/**
 * What a player is assigned to: documents (their character), whole types and
 * attributes. A part whose view says `self` opens to whoever is assigned to
 * its document or its type.
 */
function Assigned({
  world,
  templates,
  articles,
  chosen,
  onAdd,
  onRemove,
}: {
  world: WorldInfo;
  templates: Template[];
  articles: ArticleView[];
  chosen: string[];
  onAdd: (a: string) => void;
  onRemove: (a: string) => void;
}) {
  const types = new Map(templates.map((t) => [t.id, t]));
  const items: Pickable[] = [
    ...articles
      .filter((a) => types.has(a.articleType))
      .map((a) => ({
        id: `doc:${a.id.replace(/^art:/, "")}`,
        name: a.name,
        icon: types.get(a.articleType)?.icon,
        hint: types.get(a.articleType)?.name,
      })),
    ...templates.map((t) => ({ id: typeAttribute(t.id), name: `All ${t.name}`, icon: t.icon, hint: "type" })),
    ...world.roles
      .filter((a) => !PLAIN.includes(a) && !a.startsWith("doc:") && !a.startsWith("type:"))
      .map((a) => ({ id: a, name: a })),
  ];
  return (
    <div className="assigned">
      <SearchPick items={items} chosen={chosen} onAdd={onAdd} onRemove={onRemove} placeholder="Assign…" />
    </div>
  );
}

export function PlayersArea({
  world,
  me,
  templates,
  articles,
  onChanged,
  onOpen,
  onError,
}: {
  world: WorldInfo;
  me: IdentityInfo;
  templates: Template[];
  articles: ArticleView[];
  onChanged: () => void;
  /** Opens a document — the options close for it. */
  onOpen: (id: string) => void;
  onError: (e: unknown) => void;
}) {
  const [requests, setRequests] = useState<JoinRequest[]>([]);
  const [waiting, setWaiting] = useState<WaitingVersion[]>([]);
  const [id, setId] = useState("");
  const [remove, setRemove] = useState<{ id: string; name: string } | null>(null);
  const [handover, setHandover] = useState<MemberInfo | null>(null);
  const [newId, setNewId] = useState<MemberInfo | null>(null);
  /** Who takes whose place: new id → old id. */
  const takes = world.takeovers;
  const nameOf = (id: string) => world.members.find((m) => m.id === id)?.name ?? id;

  const [takingOver, setTakingOver] = useState(false);
  // `world.requests` changes when someone new asks — then the list is read again.
  useEffect(() => {
    if (!world.isGm && !world.supporter) return;
    api.joinRequests(world.id).then(setRequests).catch(onError);
  }, [world.id, world.isGm, world.supporter, world.requests]);
  // `world.waiting` changes when a version comes in or is decided.
  useEffect(() => {
    if (!world.isGm) return;
    api.waitingVersions(world.id).then(setWaiting).catch(onError);
  }, [world.id, world.isGm, world.waiting]);

  const run = async (step: Promise<unknown>) => {
    try {
      await step;
      onChanged();
    } catch (e) {
      onError(e);
    }
  };

  const invite = () =>
    run((world.mailboxes ? api.mailboxAdd(world.id, [id.trim()]) : api.memberInvite(world.id, id.trim())).then(() => setId("")));

  const admit = async () => {
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "Player id", extensions: ["awid", "json"] }],
      });
      if (typeof path !== "string") return;
      await api.memberAdd(world.id, path, ["players"]);
      onChanged();
    } catch (e) {
      onError(e);
    }
  };

  const asking = requests.length > 0 && (
    <>
      <h3>Asking to join</h3>
      <ul className="members large requests">
        {requests.map((r) => (
          <li key={r.id}>
            <div>
              <strong>{r.name}</strong>
              <small className="small">{r.fingerprint}</small>
              {takes[r.id] && <small className="accent">Takes {nameOf(takes[r.id])}'s place</small>}
            </div>
            <div className="line">
              <button className="primary compact" onClick={() => run(api.joinAnswer(world.id, r.id, true))}>
                Accept
              </button>
              <button className="compact" onClick={() => run(api.joinAnswer(world.id, r.id, false))}>Decline</button>
            </div>
          </li>
        ))}
      </ul>
    </>
  );

  const supports = (id: string) => world.supporters.includes(id);
  const removeDialog = remove && (
    <DeleteDialog
      title={`Remove ${remove.name}`}
      name={remove.name}
      warning="What they have stays with them. Nothing new reaches them."
      onCancel={() => setRemove(null)}
      onDelete={() => {
        const who = remove.id;
        setRemove(null);
        void run(api.memberRemove(world.id, who));
      }}
    />
  );

  // A supporter sees the members as the GM does: gives players roles, takes them out.
  const supporting = !world.isGm && world.supporter && world.members.some((m) => m.id !== me.id);

  if (!world.isGm)
    return (
      <div className="type-page">
        <h2>Players</h2>
        <ul className="members large">
          {!supporting && (
            <li>
              <div>
                <strong>{me.name}</strong> <span className="role-badge player">Player</span>
                {world.supporter && <> <span className="role-badge">Supporter</span></>}
                <small className="small">{world.attributes.filter((a) => !PLAIN.includes(a)).join(", ")}</small>
              </div>
              <OwnColour world={world} me={me} onChanged={onChanged} onError={onError} />
            </li>
          )}
          {supporting &&
            world.members.map((m) => {
              const extra = m.attributes.filter((a) => !PLAIN.includes(a));
              const assign = (fresh: string[]) => run(api.memberAttributesSet(world.id, m.id, fresh));
              return (
                <li key={m.id}>
                  <div>
                    <strong>{m.name}</strong>{" "}
                    <span className={`role-badge ${m.role}`}>{m.role === "gm" ? "GM" : "Player"}</span>
                    {supports(m.id) && <> <span className="role-badge">Supporter</span></>}
                    <small className="small">{m.fingerprint}</small>
                    {m.role !== "gm" ? (
                      <Assigned
                        world={world}
                        templates={templates}
                        articles={articles}
                        chosen={extra}
                        onAdd={(a) => assign([...extra, a])}
                        onRemove={(a) => assign(extra.filter((x) => x !== a))}
                      />
                    ) : (
                      extra.length > 0 && <small className="small">{extra.join(", ")}</small>
                    )}
                  </div>
                  {m.id === me.id && <OwnColour world={world} me={me} onChanged={onChanged} onError={onError} />}
                  {m.id !== me.id && m.role !== "gm" && !supports(m.id) && (
                    <DeleteButton what={m.name} onClick={() => setRemove({ id: m.id, name: m.name })} />
                  )}
                </li>
              );
            })}
        </ul>
        {asking}
        {removeDialog}
        {world.supporter && (
          <>
            <h3>Emergency</h3>
            {takingOver ? (
              <div className="line">
                <small className="small grow">You become GM. Everyone sees it.</small>
                <button className="danger compact" onClick={() => run(api.worldTakeOver(world.id).then(() => setTakingOver(false)))}>
                  Become GM
                </button>
                <button className="compact" onClick={() => setTakingOver(false)}>Cancel</button>
              </div>
            ) : (
              <button onClick={() => setTakingOver(true)}>Take the GM's place …</button>
            )}
          </>
        )}
      </div>
    );

  const chooseSupporter = (id: string, on: boolean) =>
    run(api.worldSupportersSet(world.id, on ? [...world.supporters, id] : world.supporters.filter((c) => c !== id)));

  return (
    <div className="type-page">
      <h2>Players</h2>

      <label className="switch-row">
        <input
          type="checkbox"
          checked={world.moderated}
          onChange={(e) => run(api.worldModerationSet(world.id, e.target.checked))}
        />
        <span>
          <strong>Review changes</strong>
          <small className="small">What players write waits for you before the others see it.</small>
        </span>
      </label>

      {waiting.length > 0 && (
        <>
          <h3>Waiting for review</h3>
          <ul className="members large">
            {waiting.map((w) => (
              <li key={w.session}>
                <div>
                  <strong>{w.name}</strong>
                  <small className="small">
                    {w.author} · {when(w.to)}
                  </small>
                </div>
                <button className="compact" onClick={() => onOpen(w.article)}>
                  Open
                </button>
              </li>
            ))}
          </ul>
        </>
      )}

      {asking}

      <ul className="members large">
        {world.members.map((m) => {
          const extra = m.attributes.filter((a) => !PLAIN.includes(a));
          const assign = (fresh: string[]) => run(api.memberAttributesSet(world.id, m.id, fresh));
          return (
            <li key={m.id}>
              <div>
                <strong>{m.name}</strong>{" "}
                <span className={`role-badge ${m.role}`}>{m.role === "gm" ? "GM" : "Player"}</span>
                {supports(m.id) && <> <span className="role-badge">Supporter</span></>}
                {m.id.startsWith("slot:") && <> <span className="role-badge">Not linked</span></>}
                <small className="small">{[m.slot && `mailbox ${m.slot}`, !m.id.startsWith("slot:") && m.fingerprint].filter(Boolean).join(" · ")}</small>
                {world.isGm && (m.role !== "gm" || m.id === me.id) && (
                  <Assigned
                    world={world}
                    templates={templates}
                    articles={articles}
                    chosen={extra}
                    onAdd={(a) => assign([...extra, a])}
                    onRemove={(a) => assign(extra.filter((x) => x !== a))}
                  />
                )}
                {!world.isGm && extra.length > 0 && <small className="small">{extra.join(", ")}</small>}
                {Object.entries(takes).filter(([, old]) => old === m.id).map(([fresh]) => (
                  <small key={fresh} className="accent">Moves to {fresh} as soon as it connects.</small>
                ))}
              </div>
              {m.id === me.id && <OwnColour world={world} me={me} onChanged={onChanged} onError={onError} />}
              {m.id !== me.id && (
                <div className="line">
                  {world.moderated && (
                    <button
                      className={m.trusted ? "compact on" : "compact"}
                      title={m.trusted ? "Their changes count at once" : "Their changes wait for review"}
                      onClick={() => run(api.memberTrustSet(world.id, m.id, !m.trusted))}
                    >
                      Trusted
                    </button>
                  )}
                  {m.role !== "gm" && !world.mailboxes && (
                    <button
                      className={supports(m.id) ? "compact on" : "compact"}
                      title="Lets players in; may become GM if you are gone"
                      onClick={() => chooseSupporter(m.id, !supports(m.id))}
                    >
                      Supporter
                    </button>
                  )}
                  {m.slot && !m.id.startsWith("slot:") && (
                    <button className="compact" title="Whoever writes into the mailbox next takes it" onClick={() => run(api.mailboxReset(world.id, m.id))}>
                      Unlink
                    </button>
                  )}
                  {!world.mailboxes && <button className="compact" onClick={() => setHandover(m)}>Make GM</button>}
                  {m.role !== "gm" && !world.mailboxes && (
                    <button className="compact" title="For a device set up anew without its profile" onClick={() => setNewId(m)}>
                      New ID
                    </button>
                  )}
                  <DeleteButton what={m.name} onClick={() => setRemove({ id: m.id, name: m.name })} />
                </div>
              )}
            </li>
          );
        })}
        {world.invited.map((i) => (
          <li key={i}>
            <div>
              <code>{i}</code> <span className="role-badge">Invited</span>
              {takes[i] && <small className="accent">Takes {nameOf(takes[i])}'s place</small>}
            </div>
            <DeleteButton what={i} onClick={() => run(api.memberRemove(world.id, i))} />
          </li>
        ))}
      </ul>

      <h3>Add a player</h3>
      <div className="line">
        <input
          className="grow"
          placeholder={world.mailboxes ? "Name" : "id:…"}
          value={id}
          onChange={(e) => setId(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && id.trim() && invite()}
        />
        <button className="icon" title="Add" aria-label="Add" onClick={invite} disabled={!id.trim()}>
          <Icon name="add" />
        </button>
        {!world.mailboxes && (
          <button className="icon" title="From file" aria-label="From file" onClick={admit}>
            <Icon name="folder-open" />
          </button>
        )}
      </div>
      {world.mailboxes && <small className="small muted">Share each player's folder with them alone, and “alle” with everyone.</small>}

      {removeDialog}

      {newId && (
        <NewIdDialog
          member={newId}
          onCancel={() => setNewId(null)}
          onGive={(fresh) => {
            const old = newId.id;
            setNewId(null);
            void run(api.memberReplace(world.id, old, fresh));
          }}
        />
      )}

      {handover && (
        <HandoverDialog
          world={world}
          member={handover}
          onCancel={() => setHandover(null)}
          onDone={() => {
            setHandover(null);
            onChanged();
          }}
          onError={onError}
        />
      )}
    </div>
  );
}

/**
 * One's own colour in this world — on one's name and the characters
 * assigned to one, for everyone. Signed by oneself; no GM needed.
 */
function OwnColour({ world, me, onChanged, onError }: { world: WorldInfo; me: IdentityInfo; onChanged: () => void; onError: (e: unknown) => void }) {
  const now = world.colours[`player:${me.name.toLowerCase()}`] ?? "#2fc6b2";
  // Kept once chosen — not at every step while the picker is dragged.
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const el = input.current;
    if (!el) return;
    const chosen = () => {
      if (el.value !== now) api.memberColourSet(world.id, el.value).then(onChanged).catch(onError);
    };
    el.addEventListener("change", chosen);
    return () => el.removeEventListener("change", chosen);
  }, [world.id, now]);
  return (
    <label className="own-colour" title="Your colour — on your name and your characters, for everyone">
      <input ref={input} type="color" key={now} defaultValue={now} />
      <span className="small">Your colour</span>
    </label>
  );
}

/** A player's new id: what was theirs goes to it, the old one leaves. */
function NewIdDialog({ member, onCancel, onGive }: { member: MemberInfo; onCancel: () => void; onGive: (id: string) => void }) {
  const [id, setId] = useState("");
  const valid = /^id:[0-9a-fA-F]{16}$/.test(id.trim());
  return (
    <div className="dialog-backdrop" onClick={onCancel}>
      <div className="delete-dialog" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
        <h3>New ID for {member.name}</h3>
        <p className="small">
          What is assigned to {member.name} goes to the new ID. The old one leaves the world.
        </p>
        <label>
          The new ID
          <input
            autoFocus
            value={id}
            placeholder="id:…"
            onChange={(e) => setId(e.target.value)}
            onKeyDown={(e) => (e.key === "Escape" ? onCancel() : e.key === "Enter" && valid && onGive(id.trim()))}
          />
        </label>
        <div className="line">
          <button className="primary" disabled={!valid} onClick={() => onGive(id.trim())}>
            Give
          </button>
          <button onClick={onCancel}>Cancel</button>
        </div>
      </div>
    </div>
  );
}

function HandoverDialog({
  world,
  member,
  onCancel,
  onDone,
  onError,
}: {
  world: WorldInfo;
  member: MemberInfo;
  onCancel: () => void;
  onDone: () => void;
  onError: (e: unknown) => void;
}) {
  const [input, setInput] = useState("");
  const matches = input.trim() === world.name.trim();

  return (
    <div className="dialog-backdrop" onClick={onCancel}>
      <div className="delete-dialog" role="dialog" aria-modal="true" onClick={(e) => e.stopPropagation()}>
        <h3>
          Make {member.name} the GM
        </h3>
        <p className="small">
          {member.name} gets the whole world. You stay in it as a player and can no longer change
          types or members.
        </p>
        <label>
          Type the world's name to confirm:
          <input
            autoFocus
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder={world.name}
            onKeyDown={(e) => e.key === "Escape" && onCancel()}
          />
        </label>
        <div className="line">
          <button
            className="primary"
            disabled={!matches}
            onClick={() => api.gmHandover(world.id, member.id).then(onDone).catch(onError)}
          >
            Make GM
          </button>
          <button onClick={onCancel}>Cancel</button>
        </div>
      </div>
    </div>
  );
}
