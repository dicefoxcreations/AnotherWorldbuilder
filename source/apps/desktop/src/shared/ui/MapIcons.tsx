import type { ReactNode } from "react";

/**
 * The markers a pin on a map may be (`MARKERS` in `GraphText`) — drawn
 * here, line by line, on a 24 by 24 grid, in the colour of the text. `pin`
 * is the plain pin.
 */
export { MARKERS } from "../GraphText";

const SHAPES: Record<string, ReactNode> = {
  pin: (
    <>
      <path d="M12 21s-6-5.6-6-11a6 6 0 0 1 12 0c0 5.4-6 11-6 11z" />
      <circle cx="12" cy="10" r="2" />
    </>
  ),
  door: (
    <>
      <path d="M6 21V4h12v17M4 21h16" />
      <circle cx="15" cy="12.5" r="1" />
    </>
  ),
  ladder: <path d="M7 3v18M17 3v18M7 7h10M7 11h10M7 15h10M7 19h10" />,
  stairs: <path d="M3 20h5v-5h5v-5h5V5h3" />,
  chest: <path d="M3 10V8a3 3 0 0 1 3-3h12a3 3 0 0 1 3 3v2M3 10h18v10H3zM11 12h2v4h-2z" />,
  skull: (
    <>
      <path d="M12 3a8 8 0 0 0-8 8c0 3 2 5 3 5.5V20h10v-3.5c1-.5 3-2.5 3-5.5a8 8 0 0 0-8-8zM10 20v-2M14 20v-2" />
      <circle cx="9" cy="11" r="1.6" />
      <circle cx="15" cy="11" r="1.6" />
    </>
  ),
  castle: <path d="M4 21V8h3v3h3V8h4v3h3V8h3v13zM10 21v-4a2 2 0 0 1 4 0v4" />,
  tower: <path d="M8 21V8h8v13M7 8V4h2v2h2V4h2v2h2V4h2v4M11 21v-3h2v3M11 12h2" />,
  house: <path d="M3 11l9-7 9 7M5 10v11h14V10M10 21v-6h4v6" />,
  camp: <path d="M3 20L12 4l9 16zM12 4v16M9 20l3-6 3 6" />,
  cave: <path d="M2 20c1-8 5-14 10-14s9 6 10 14zM8 20c0-4 2-7 4-7s4 3 4 7" />,
  ship: <path d="M3 15h18l-3 5H6zM12 3v12M12 4l6 8h-6M12 6l-5 6h5" />,
  anchor: (
    <>
      <circle cx="12" cy="5" r="2" />
      <path d="M12 7v14M5 13a7 7 0 0 0 14 0M8 10h8" />
    </>
  ),
  tree: <path d="M12 3L5 13h4l-3 5h12l-3-5h4zM12 18v3" />,
  mountain: <path d="M2 20L9 7l4 7 2-3 7 9zM7.5 10l1.5 2 1.5-1.5" />,
  flag: <path d="M5 21V3M5 4h12l-3 4 3 4H5" />,
  star: <path d="M12 3l2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1-4.4-4.3 6.1-.9z" />,
  sword: <path d="M14 4h6v6L9 21l-3-3zM6 15l3 3M4 17l3 3M3 21l2-2" />,
  eye: (
    <>
      <path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z" />
      <circle cx="12" cy="12" r="3" />
    </>
  ),
  question: (
    <>
      <path d="M9 9a3 3 0 1 1 4 2.8c-.7.3-1 .8-1 1.5V15" />
      <circle cx="12" cy="19" r="1" />
    </>
  ),
};

/** A marker drawn: `door`, `ladder` … — an unknown name is the pin. */
export function MarkerIcon({ name, size = 16 }: { name: string; size?: number }) {
  return (
    <svg
      className="marker-icon"
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      {SHAPES[name] ?? SHAPES.pin}
    </svg>
  );
}
