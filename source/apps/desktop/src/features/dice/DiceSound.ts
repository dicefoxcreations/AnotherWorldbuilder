import rolling from "./dice-roll.mp3";

/**
 * The sound of dice on a table — a recording (CC0, see
 * docs/development/licenses.md), played on this device only; a shared roll
 * sounds on each device that shows it, nothing goes over the network.
 *
 * Through Web Audio, not an <audio> element: WebKitGTK plays no media from
 * the app's own scheme, but it fetches and decodes it.
 */
let context: AudioContext | null = null;
let decoded: Promise<AudioBuffer> | null = null;

function recording(): Promise<AudioBuffer> {
  context ??= new AudioContext();
  const ctx = context;
  decoded ??= fetch(rolling)
    .then((r) => r.arrayBuffer())
    .then((bytes) => ctx.decodeAudioData(bytes))
    .catch((e) => {
      decoded = null;
      throw e;
    });
  return decoded;
}

export function sound(after = 0) {
  setTimeout(() => {
    recording()
      .then(async (buffer) => {
        const ctx = context!;
        if (ctx.state === "suspended") await ctx.resume();
        const source = ctx.createBufferSource();
        source.buffer = buffer;
        const volume = ctx.createGain();
        volume.gain.value = 0.7;
        source.connect(volume).connect(ctx.destination);
        source.start();
      })
      // No sound here: the dice still roll.
      .catch(() => {});
  }, Math.max(0, after));
}
