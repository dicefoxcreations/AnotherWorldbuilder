/** Native mobile builds use Android's sandbox and package updates. */
export const isMobile = typeof navigator !== "undefined" && /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
