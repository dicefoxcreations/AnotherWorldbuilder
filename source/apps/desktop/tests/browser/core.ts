/**
 * The bridge to the test bench.
 *
 * In the test bench it stands in the place of Tauri. Behind it runs the same
 * Rust code as in the app — only over HTTP instead of the Tauri bridge. That
 * is why the test bench shows real behaviour and not what a stub thinks is
 * right.
 *
 * If the server is not running, the page says so at the top — silently wrong
 * answers would be worse than none at all.
 */

const SERVER = "http://127.0.0.1:5320";

export let lastError: string | null = null;

/**
 * Which types exist — id and name.
 *
 * The text carries the name of a type, what is stored is its id. In the app
 * Rust reads the directory from the world; there is none here, so the page
 * fills it in.
 */
export let directory: { id: string; name: string }[] = [];

export function setDirectory(types: { id: string; name: string }[]) {
  directory = types;
}

/**
 * Answers for commands the core does not have — a world's versions, say.
 * Only for stages that show something; set by the stage itself. A function
 * answers from the arguments.
 */
const fixtures = new Map<string, unknown>();

export function setFixture(cmd: string, value: unknown) {
  fixtures.set(cmd, value);
}

// The dice of the open world as a device keeps them — in memory here.
type Roll = { id: string; mark?: string };
let dice: { rolls: Roll[]; favorites: unknown[] } = { rolls: [], favorites: [] };
fixtures.set("dice_log", () => dice);
fixtures.set("dice_log_add", ({ roll }: { roll: Roll }) =>
  roll.mark === "clear" || dice.rolls.some((r) => r.id === roll.id) ? dice : (dice = { ...dice, rolls: [...dice.rolls, roll] }),
);
fixtures.set("dice_log_clear", () => (dice = { ...dice, rolls: [] }));
fixtures.set("dice_favorites_set", ({ favorites }: { favorites: unknown[] }) => (dice = { ...dice, favorites }));
fixtures.set("dice_rolls", []);
// Texts as written — the stages name documents by name already.
fixtures.set("texts_written", ({ texts }: { texts: string[] }) => texts);

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (fixtures.has(cmd)) {
    const fixture = fixtures.get(cmd);
    return (typeof fixture === "function" ? fixture(args ?? {}) : fixture) as T;
  }
  const response = await fetch(SERVER, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ cmd, args: { ...(args ?? {}), directory } }),
  }).catch((e) => {
    lastError = `Test bench not reachable (${SERVER}). Is \`cargo run -p aw-testbench\` running? — ${e}`;
    throw new Error(lastError);
  });

  const data = await response.json();
  if (data.error) throw new Error(data.error);
  return data.value as T;
}
