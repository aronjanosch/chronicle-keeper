// Helpers shared by the Atlas side panel and preview cards for a page's
// headings. No imports.

// H1–H3 of a page body, fenced code skipped — the anchors a pin can point at.
export function pageHeadings(md) {
  const out = [];
  let fence = false;
  for (const raw of (md || '').split('\n')) {
    const line = raw.trimEnd();
    if (/^(```|~~~)/.test(line.trim())) { fence = !fence; continue; }
    if (fence) continue;
    const m = /^(#{1,3})\s+(.+?)\s*#*$/.exec(line);
    if (m) out.push(m[2].trim());
  }
  return out;
}

// Scroll a rendered page to its heading and flash it; false if it isn't there.
export function flashHeading(root, heading) {
  const el = [...(root?.querySelectorAll('h1,h2,h3') || [])].find((h) => h.textContent.trim() === heading);
  if (!el) return false;
  el.scrollIntoView({ block: 'start', behavior: 'smooth' });
  el.animate([{ background: 'var(--burgundy-50)' }, { background: 'transparent' }], { duration: 1600 });
  return true;
}
