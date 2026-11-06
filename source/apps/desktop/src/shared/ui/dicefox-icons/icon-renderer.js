import { ICON_DATA, ICON_ALIASES } from './icon-data.js';

export const iconNames = Object.freeze(Object.keys(ICON_DATA));

/** Shared renderer for standalone SVGs, the review page and the React adapter. */
export function iconParts(name, options = {}) {
  const key = ICON_ALIASES[name] ?? name;
  const item = ICON_DATA[key];
  if (!item) throw new RangeError(`Unknown DiceFox icon: ${name}`);
  const { size = 24, mode = 'profile', shading = true } = options;
  if (!Number.isFinite(size) || size <= 0) throw new RangeError('Positive icon size required');
  if (mode !== 'mono' && mode !== 'profile') throw new RangeError('Use mono or profile');
  const compact = options.compact ?? size <= 20;
  const primary = options.primary ?? 'currentColor';
  const accent = mode === 'mono' ? primary : options.accent ?? 'var(--dicefox-icon-accent, var(--accent, currentColor))';
  const outer = compact ? 1.5 : 1.35;
  const children = [];
  if (shading) for (const face of item.planes ?? []) {
    children.push({ tag: 'path', attrs: { d: face.d, fill: primary, 'fill-opacity': face.tone, stroke: 'none', 'data-plane': true } });
  }
  for (const [index, path] of item.paths.entries()) {
    if (compact && path.fine) continue;
    const role = path.role ?? (index === 0 ? 'outline' : 'inner');
    let weight = path.weight ?? (role === 'outline' ? 1.35 : path.fine ? .8 : 1);
    if (compact) weight = role === 'outline' ? 1.5 : Math.max(weight, 1.05);
    children.push({ tag: 'path', attrs: {
      d: path.d, stroke: path.accent ? accent : primary, 'stroke-width': weight,
      'data-role': role, ...(path.accent ? { 'data-accent': true } : {}), ...(path.fine ? { 'data-fine': true } : {}),
    } });
  }
  for (const dot of item.dots ?? []) {
    if (compact && dot.fine) continue;
    children.push({ tag: 'ellipse', attrs: {
      cx: dot.cx, cy: dot.cy, rx: dot.rx, ry: dot.ry,
      fill: dot.accent ? accent : primary, stroke: 'none', 'data-dot': true,
      ...(dot.accent ? { 'data-accent': true } : {}), ...(dot.fine ? { 'data-fine': true } : {}),
    } });
  }
  return { attrs: {
    xmlns: 'http://www.w3.org/2000/svg', viewBox: '0 0 24 24', width: size, height: size,
    fill: 'none', stroke: primary, 'stroke-width': outer, 'stroke-linecap': 'round', 'stroke-linejoin': 'round',
    'aria-hidden': true, 'data-dicefox-icon': name, 'data-color-mode': mode,
  }, children };
}

const escapeXml = (value) => String(value).replace(/[&<>"']/g, char => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&apos;' })[char]);
const attributes = (attrs) => Object.entries(attrs).map(([key, value]) => ` ${key}="${escapeXml(value)}"`).join('');

/** All input paths are authored library data; attributes are escaped. */
export function iconMarkup(name, options = {}) {
  const { attrs, children } = iconParts(name, options);
  return '<svg' + attributes(attrs) + '>' + children.map(({ tag, attrs }) => '<' + tag + attributes(attrs) + '/>').join('') + '</svg>';
}
