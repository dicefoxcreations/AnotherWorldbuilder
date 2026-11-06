import { createElement, type SVGProps } from 'react';
import { iconParts, type DiceFoxColorMode, type DiceFoxIconName } from './icon-renderer.js';

export type { DiceFoxIconName, DiceFoxColorMode } from './icon-renderer.js';
export type DiceFoxIconProps = Omit<SVGProps<SVGSVGElement>, 'name' | 'children'> & {
  name: DiceFoxIconName;
  size?: number;
  mode?: DiceFoxColorMode;
  shading?: boolean;
  title?: string;
};

const reactAttrs = (attrs: Record<string, string | number | boolean>) => Object.fromEntries(
  Object.entries(attrs).map(([key, value]) => [
    key.startsWith('aria-') || key.startsWith('data-') ? key : key.replace(/-([a-z])/g, (_, letter: string) => letter.toUpperCase()), value,
  ]),
);

/** Uses the app's existing text and --accent colours; size selects optical detail. */
export function DiceFoxIcon({ name, size = 24, mode = 'profile', shading = true, title, className, ...props }: DiceFoxIconProps) {
  const { attrs, children } = iconParts(name, { size, mode, shading });
  const labelled = Boolean(title || props['aria-label'] || props['aria-labelledby']);
  return createElement('svg', {
    ...reactAttrs(attrs), ...props, className: ['df-icon', className].filter(Boolean).join(' '),
    'aria-hidden': labelled ? undefined : true, 'aria-label': title ?? props['aria-label'], role: labelled ? 'img' : undefined,
  }, title ? createElement('title', null, title) : null,
  ...children.map(({ tag, attrs }, key) => createElement(tag, { ...reactAttrs(attrs), key })));
}
