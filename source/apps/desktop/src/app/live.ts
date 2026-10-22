/**
 * How fast this device keeps up with the others.
 *
 * Normally every two seconds is plenty. While someone else writes in the
 * same document one wants to see it almost as it is typed — then faster.
 */
export const pace = {
  /** The network is on — only then can anyone else be here at all. */
  network: false,
  /** Someone else is in the document being edited here. */
  together: false,
};

export const SLOW_MS = 2000;
export const FAST_MS = 400;
