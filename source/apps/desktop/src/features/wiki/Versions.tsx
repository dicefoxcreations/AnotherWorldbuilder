import { useEffect, useState } from "react";
import { api, type Version } from "../../api/client";
import { DeleteButton, DeleteDialog } from "../../shared/ui/Delete";

/**
 * The versions of a document: every editing session — from going into edit
 * mode until leaving it — with who wrote, when, and what.
 *
 * Nothing is ever overwritten. A version chosen here stands beside the
 * document as it is now — in the document itself, see `DocumentPage`: what
 * is gone since red, what is new green.
 *
 * Written offline, a version may come in days after it was written: it then
 * stands at the time it was written, marked with when it arrived. Two
 * versions written at the same time on the same lines, by people who did not
 * see each other, are marked: both stand in the text, and someone should
 * look.
 *
 * Under review, a player's version waits until the GM takes it in or turns
 * it down. Chosen, it stands beside the document as the text would be with
 * it.
 *
 * The GM may delete older versions; the newest stays. How many stay at most
 * is set in the options (Basics) — beyond that the oldest go by themselves.
 */

/** Longer between writing and arriving than this: written offline. */
const LATE_MS = 10 * 60 * 1000;

export const late = (v: Version) => v.arrived - v.to > LATE_MS;

export function VersionsPanel({
  world,
  article,
  revision,
  chosen,
  fresh,
  mayRevert,
  gm,
  onChosen,
  onRevert,
  onVerdict,
  onDelete,
  onError,
}: {
  world: string;
  article: string;
  /** Changes whenever the document does — then the list is read anew. */
  revision: unknown;
  /** The version open beside the document, if any. */
  chosen: string | null;
  /** Versions that came in since one last looked. */
  fresh: string[];
  /** Whether the chosen version can be taken back by the reader. */
  mayRevert: boolean;
  /** The GM decides about waiting versions. */
  gm: boolean;
  onChosen: (session: string | null) => void;
  onRevert: (session: string) => void;
  onVerdict: (session: string, accepted: boolean) => void;
  onDelete: (session: string) => void;
  onError: (e: unknown) => void;
}) {
  const [versions, setVersions] = useState<Version[] | null>(null);
  const [deleting, setDeleting] = useState<Version | null>(null);

  const load = () => api.articleVersions(world, article).then(setVersions).catch(onError);
  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [world, article, revision]);

  const name = (s: string) => {
    const v = versions?.find((x) => x.session === s);
    return v ? `${v.author}, ${when(v.from)}` : "another version";
  };

  return (
    <>
      <aside className="versions">
        <h3>Versions</h3>
        {versions === null ? (
          <p className="small">Loading …</p>
        ) : versions.length === 0 ? (
          <p className="small">No versions yet — every editing session becomes one.</p>
        ) : (
          <ol>
            {versions.map((v) => (
              <li key={v.session} className={chosen === v.session ? "on" : ""}>
                <button className="version" onClick={() => onChosen(chosen === v.session ? null : v.session)}>
                  <strong>
                    {v.author}
                    {fresh.includes(v.session) && <span className="fresh">new</span>}
                  </strong>
                  <small>
                    written {when(v.from)}
                    {v.to - v.from >= 60_000 && ` – ${clock(v.to)}`}
                  </small>
                  {late(v) && <small className="late">arrived {when(v.arrived)}</small>}
                  {v.overlaps.length > 0 && (
                    <small className="overlap" title={`Written at the same time on the same lines as ${v.overlaps.map(name).join(", ")}`}>
                      ⚠ overlaps
                    </small>
                  )}
                  {v.stale && <small className="overlap">not in the text yet</small>}
                  {v.review === "waiting" && <small className="waiting">waits for review</small>}
                  {v.review === "rejected" && <small className="rejected">turned down</small>}
                </button>
                {chosen === v.session && (
                  <div className="version-actions">
                    {gm && v.review === "waiting" && (
                      <>
                        <button className="primary compact" onClick={() => onVerdict(v.session, true)}>
                          Take in
                        </button>
                        <button className="danger compact" onClick={() => onVerdict(v.session, false)}>
                          Turn down
                        </button>
                      </>
                    )}
                    {mayRevert && !v.stale && v.review === "counted" && (
                      <button
                        className="compact"
                        onClick={() => onRevert(v.session)}
                        title="Takes back what this version changed. What others wrote since stays."
                      >
                        ↶ Take back
                      </button>
                    )}
                    {gm && v.removable && <DeleteButton what="this version" onClick={() => setDeleting(v)} />}
                  </div>
                )}
              </li>
            ))}
          </ol>
        )}
      </aside>
      {deleting && (
        <DeleteDialog
          title={`Delete ${deleting.author}'s version`}
          name={deleting.author}
          warning="Gone for good, on every device."
          onCancel={() => setDeleting(null)}
          onDelete={() => {
            onDelete(deleting.session);
            setDeleting(null);
          }}
        />
      )}
    </>
  );
}

export function when(ms: number): string {
  return new Date(ms).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

function clock(ms: number): string {
  return new Date(ms).toLocaleTimeString(undefined, { timeStyle: "short" });
}
