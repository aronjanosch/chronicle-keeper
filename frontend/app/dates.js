// The one date formatter: '20 May 2026', relative ('3d ago') under a week.
// In-world (fictional calendar) dates are ISO-like strings shown verbatim, labelled.
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
const DAY = 86400000;

function parse(v) {
  if (v == null || v === '') return null;
  if (typeof v === 'number') return { d: new Date(v < 1e12 ? v * 1000 : v), dateOnly: false };
  const s = String(v);
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(s);
  if (m) return { d: new Date(+m[1], +m[2] - 1, +m[3]), dateOnly: true };
  const d = new Date(s);
  return Number.isNaN(d.getTime()) ? null : { d, dateOnly: false };
}

const abs = (d) => `${d.getDate()} ${MONTHS[d.getMonth()]} ${d.getFullYear()}`;
const hm = (d) => `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;

export function fmtDate(v, now = Date.now()) {
  const p = parse(v);
  if (!p) return v ? String(v) : '';
  const { d, dateOnly } = p;
  if (dateOnly) {
    const t = new Date(now);
    const days = Math.round((new Date(t.getFullYear(), t.getMonth(), t.getDate()) - d) / DAY);
    if (days === 0) return 'Today';
    if (days > 0 && days < 7) return `${days}d ago`;
    return abs(d);
  }
  return relative(d, now) || abs(d);
}

export function fmtDateTime(v, now = Date.now()) {
  const p = parse(v);
  if (!p) return v ? String(v) : '';
  return relative(p.d, now) || (p.dateOnly ? abs(p.d) : `${abs(p.d)}, ${hm(p.d)}`);
}

function relative(d, now) {
  const diff = now - d.getTime();
  if (diff < 0 || diff >= 7 * DAY) return '';
  if (diff < 90000) return 'just now';
  if (diff < 3600000) return `${Math.round(diff / 60000)}m ago`;
  if (diff < DAY) return `${Math.round(diff / 3600000)}h ago`;
  return `${Math.floor(diff / DAY)}d ago`;
}

export const fmtInWorld = (iso) => (iso ? `In-world ${iso}` : '');
