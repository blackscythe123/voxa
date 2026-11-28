/* Open graph safe zone: guides and check. Generic, no project content.
   Include with <script src="tools/og-guides.js"></script> (path relative to the page).
   Zone defaults to GitHub's repository template at 1280 by 640: x 80 to 1200, y 80 to 560
   (its 40 pt border at 2x). Override with window.SAFE_ZONE = {x0, y0, x1, y1} before this script.
   ?guides   draws the template's red guide lines, tints the margin and lists any violation.
   window.checkSafe() returns the [data-safe] elements that are visible and outside the zone. */
(function () {
  const Z = window.SAFE_ZONE || { x0: 80, y0: 80, x1: 1200, y1: 560 };
  window.SAFE_ZONE = Z;
  const opacityOf = el => { let o = 1; for (let e = el; e && e.nodeType === 1; e = e.parentElement) { const s = getComputedStyle(e); if (s.display === 'none' || s.visibility === 'hidden') return 0; o *= +s.opacity; } return o; };
  window.checkSafe = function (tol = 0.5) {
    const bad = [];
    document.querySelectorAll('[data-safe]').forEach(el => {
      if (opacityOf(el) < 0.02) return;
      const r = el.getBoundingClientRect();
      if (!r.width && !r.height) return;
      if (r.left < Z.x0 - tol || r.top < Z.y0 - tol || r.right > Z.x1 + tol || r.bottom > Z.y1 + tol)
        bad.push({ el: el.dataset.safe || el.id || el.className || el.tagName, left: +r.left.toFixed(1), top: +r.top.toFixed(1), right: +r.right.toFixed(1), bottom: +r.bottom.toFixed(1) });
    });
    return bad;
  };
  if (!new URLSearchParams(location.search).has('guides')) return;
  const W = document.documentElement.clientWidth || 1280, H = document.documentElement.clientHeight || 640;
  const g = document.createElement('div');
  g.style.cssText = 'position:fixed;inset:0;pointer-events:none;z-index:2147483647';
  const tint = 'rgba(214,40,57,.10)', line = '#B3242F';
  g.innerHTML = `<svg width="${W}" height="${H}" style="position:absolute;inset:0">
    <path d="M0 0H${W}V${H}H0Z M${Z.x0} ${Z.y0}V${Z.y1}H${Z.x1}V${Z.y0}Z" fill="${tint}" fill-rule="evenodd"/>
    <g stroke="${line}" stroke-width="3"><path d="M${Z.x0} 0V${H}M${Z.x1} 0V${H}M0 ${Z.y0}H${W}M0 ${Z.y1}H${W}"/></g></svg>
    <div id="og-guides-report" style="position:absolute;left:${Z.x0 + 6}px;top:${Z.y1 + 8}px;font:600 13px system-ui;color:${line}"></div>`;
  const add = () => { document.body.appendChild(g); const upd = () => { const b = window.checkSafe(); const rep = g.querySelector('#og-guides-report');
    rep.textContent = b.length ? 'Outside the safe zone: ' + b.map(x => x.el).join(', ') : 'Safe zone: every data-safe element is inside';
    if (b.length) console.error('safe zone', b); }; setTimeout(upd, 300); setInterval(upd, 500); };
  document.body ? add() : addEventListener('DOMContentLoaded', add);
})();
