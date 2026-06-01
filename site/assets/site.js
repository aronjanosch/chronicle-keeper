// Chronicle Keeper docs — tiny progressive-enhancement helpers. No framework.

// ── Mobile nav toggle ─────────────────────────────────────────────
document.addEventListener('click', (e) => {
  const t = e.target.closest('.nav-toggle');
  if (t) document.querySelector('.topnav nav')?.classList.toggle('open');
});

// ── Copy buttons on code blocks ───────────────────────────────────
document.querySelectorAll('pre').forEach((pre) => {
  const code = pre.querySelector('code');
  if (!code) return;
  const btn = document.createElement('button');
  btn.className = 'copy-btn';
  btn.type = 'button';
  btn.textContent = 'Copy';
  btn.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(code.innerText.trim());
      btn.textContent = 'Copied';
      btn.classList.add('copied');
      setTimeout(() => { btn.textContent = 'Copy'; btn.classList.remove('copied'); }, 1600);
    } catch { /* clipboard blocked — ignore */ }
  });
  pre.appendChild(btn);
});

// ── Auto-build the "On this page" TOC + scrollspy ─────────────────
const toc = document.querySelector('.docs-toc');
const prose = document.querySelector('.prose');
if (toc && prose) {
  const heads = [...prose.querySelectorAll('h2[id]')];
  if (heads.length) {
    const list = document.createElement('div');
    list.innerHTML = '<h6>On this page</h6>';
    heads.forEach((h) => {
      const a = document.createElement('a');
      a.href = '#' + h.id;
      a.textContent = h.textContent.replace('#', '').trim();
      list.appendChild(a);
    });
    toc.appendChild(list);

    const links = [...toc.querySelectorAll('a')];
    const spy = new IntersectionObserver((entries) => {
      entries.forEach((en) => {
        if (en.isIntersecting) {
          links.forEach((l) => l.classList.toggle('is-active', l.getAttribute('href') === '#' + en.target.id));
        }
      });
    }, { rootMargin: '-80px 0px -70% 0px' });
    heads.forEach((h) => spy.observe(h));
  } else {
    toc.remove();
  }
}

// ── Clickable anchor links on headings ────────────────────────────
if (prose) {
  prose.querySelectorAll('h2[id], h3[id]').forEach((h) => {
    const a = document.createElement('a');
    a.className = 'anchor';
    a.href = '#' + h.id;
    a.textContent = '#';
    a.setAttribute('aria-label', 'Link to this section');
    h.appendChild(a);
  });
}

// ── Highlight the current page in the docs sidebar ────────────────
{
  const here = location.pathname.split('/').pop() || 'index.html';
  document.querySelectorAll('.docs-sidebar a').forEach((a) => {
    const target = (a.getAttribute('href') || '').split('/').pop().split('#')[0];
    if (target === here) a.classList.add('is-active');
  });
}

// ── Year in footer ────────────────────────────────────────────────
document.querySelectorAll('[data-year]').forEach((el) => {
  el.textContent = new Date().getFullYear();
});
