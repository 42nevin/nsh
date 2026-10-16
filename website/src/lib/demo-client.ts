/** The Preview / Source switch of demo panels (markup from `demo.js`). */
export function initDemos(root: ParentNode = document) {
  for (const demo of root.querySelectorAll<HTMLElement>('.demo[data-view]')) {
    const tabs = [...demo.querySelectorAll<HTMLButtonElement>('[data-demo-view]')];
    const show = (view: string, focus = false) => {
      demo.dataset.view = view;
      for (const tab of tabs) {
        const selected = tab.dataset.demoView === view;
        tab.setAttribute('aria-selected', String(selected));
        tab.tabIndex = selected ? 0 : -1;
        const panel = document.getElementById(tab.getAttribute('aria-controls') ?? '');
        // Not `hidden`: on wide panels CSS shows both, which `hidden` (an
        // !important preflight rule) would forbid.
        if (panel) panel.toggleAttribute('data-active', selected);
        if (selected && focus) tab.focus();
      }
    };
    for (const tab of tabs) {
      tab.addEventListener('click', () => show(tab.dataset.demoView!));
      tab.addEventListener('keydown', (event) => {
        if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
        event.preventDefault();
        const at = tabs.indexOf(tab);
        const next = tabs[(at + (event.key === 'ArrowRight' ? 1 : tabs.length - 1)) % tabs.length];
        show(next.dataset.demoView!, true);
      });
    }
  }
}
