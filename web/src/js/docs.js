// SPDX-License-Identifier: AGPL-3.0-or-later
/* The contents rail's current-section mark.
 *
 * An IntersectionObserver rather than a scroll handler: the browser does the
 * geometry, so there is no per-frame work and nothing to throttle. The root
 * margin pins the trigger line a little below the sticky header and well above
 * the fold, which is what makes the mark move when a heading passes the top of
 * the reading area rather than when it enters the viewport at the bottom.
 *
 * Everything here is decoration. With the script absent the rail is still a list
 * of working anchors, which is the whole navigation — so nothing below needs a
 * fallback.
 */
(() => {
  const links = [...document.querySelectorAll('.docs-nav a[href^="#"]')];
  if (!links.length || typeof IntersectionObserver !== 'function') return;

  const byId = new Map(links.map((a) => [a.getAttribute('href')?.slice(1), a]));
  const sections = [...document.querySelectorAll('.doc-section[id]')].filter((s) => byId.has(s.id));
  if (!sections.length) return;

  /** Sections currently crossing the trigger band, in document order. */
  const active = new Set();

  const mark = () => {
    // The topmost section in the band is the one being read. When a boundary
    // falls inside the band both neighbours are active: the earlier one is the
    // section the trigger line is actually sitting in, and the later one is the
    // heading arriving from below — which is not what you are reading yet.
    let current = null;
    for (const section of sections) {
      if (active.has(section.id)) {
        current = section.id;
        break;
      }
    }
    // Nothing in the band — before the first heading, or mid-way through a
    // section longer than the band. Keep whatever was marked.
    if (current === null) return;
    for (const [id, link] of byId) {
      if (id === current) link.setAttribute('aria-current', 'true');
      else link.removeAttribute('aria-current');
    }
  };

  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) active.add(entry.target.id);
        else active.delete(entry.target.id);
      }
      mark();
    },
    { rootMargin: '-88px 0px -70% 0px', threshold: 0 },
  );

  for (const section of sections) observer.observe(section);

  // A click marks immediately, so the rail responds on the press rather than
  // after the smooth scroll has finished arriving.
  for (const link of links) {
    link.addEventListener('click', () => {
      for (const other of links) other.removeAttribute('aria-current');
      link.setAttribute('aria-current', 'true');
    });
  }
})();
