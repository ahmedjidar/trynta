// SPDX-License-Identifier: AGPL-3.0-or-later
/* Specification disclosure rows.
 *
 * One open at a time. The reveal runs on max-height because the content height
 * is not known until the text wraps — a fixed cap either truncates a rewrapped
 * row on a narrow viewport or makes short rows snap open. So it is measured.
 *
 * The teaser is taken out of flow when a row opens (see .spec-row[data-open]
 * in site.css), which promotes the body's first line to the value column's
 * first line — that is what keeps it level with the property name at any width.
 */
(() => {
  const rows = [...document.querySelectorAll('.spec-row')];
  if (!rows.length) return;

  const sync = () => {
    rows.forEach((row) => {
      const detail = row.querySelector('.spec-detail');
      const inner = detail?.firstElementChild;
      if (!inner) return;
      const open = row.dataset.open === 'true';
      detail.style.maxHeight = open ? `${inner.scrollHeight}px` : '0px';
      row.querySelector('.spec-head')?.setAttribute('aria-expanded', String(open));
    });
  };

  rows.forEach((row) => {
    row.querySelector('.spec-head')?.addEventListener('click', () => {
      const wasOpen = row.dataset.open === 'true';
      rows.forEach((r) => {
        r.dataset.open = 'false';
      });
      row.dataset.open = wasOpen ? 'false' : 'true';
      sync();
    });
  });

  sync();
  // Re-measure whenever the wrap changes: rotate, resize, or the webfont
  // landing after first paint.
  addEventListener('resize', sync);
  document.fonts?.ready.then(sync);
})();
