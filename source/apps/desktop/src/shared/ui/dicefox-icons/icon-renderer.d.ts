import { ICON_DATA, ICON_ALIASES } from './icon-data.js';
export type DiceFoxIconName = keyof typeof ICON_DATA | keyof typeof ICON_ALIASES;
export type DiceFoxColorMode = 'mono' | 'profile';
export type IconOptions = { size?: number; mode?: DiceFoxColorMode; shading?: boolean; compact?: boolean; primary?: string; accent?: string };
export type IconPart = { tag: 'path' | 'ellipse'; attrs: Record<string, string | number | boolean> };
export const iconNames: readonly DiceFoxIconName[];
export function iconParts(name: DiceFoxIconName, options?: IconOptions): { attrs: Record<string, string | number | boolean>; children: IconPart[] };
export function iconMarkup(name: DiceFoxIconName, options?: IconOptions): string;
