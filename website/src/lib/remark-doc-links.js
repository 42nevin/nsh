import { dirname, relative, resolve, sep } from 'node:path';

// Pages link to each other as files, the way they read on GitHub:
// `[List](../elements/list.md)`, `[signals](./reactivity.md#signals)`. The
// content directories mirror the routes — `elements/list.md` is
// `/elements/list`, `zh-CN/docs/x.md` is `/zh-CN/docs/x` — so a link resolves to
// the file it names relative to the page, and the route is that file's path
// without `.md` (`index` dropped). A Chinese page linking `../elements/list.md`
// therefore stays in Chinese. A link that leaves `website/` points at the file
// on GitHub instead (`../../docs/dev/architecture.md`).

const SITE_ROOT = resolve(process.cwd());
const REPO_ROOT = resolve(SITE_ROOT, '..');
const ASSET = /\.(?:png|jpe?g|gif|svg|webp|avif|ico|pdf|zip|gz|txt|json)$/i;

function isExternal(url) {
  return /^[a-z][a-z\d+.-]*:/i.test(url) || url.startsWith('//');
}

export function remarkDocLinks({ base = '/', repo = 'https://github.com/longbridge/native-shell' } = {}) {
  const prefix = base.replace(/\/+$/, '');

  return (tree, file) => {
    const from = file?.path ?? file?.history?.[0];
    if (!from) return;
    const fromDir = dirname(resolve(from));

    const rewrite = (node) => {
      const url = node.url;
      if (!url || isExternal(url) || url.startsWith('#') || url.startsWith('/')) return;

      const hashAt = url.search(/[?#]/);
      const target = hashAt === -1 ? url : url.slice(0, hashAt);
      const hash = hashAt === -1 ? '' : url.slice(hashAt);
      if (!target || ASSET.test(target)) return;

      const absolute = resolve(fromDir, target);
      const rel = relative(SITE_ROOT, absolute);
      if (rel.startsWith('..')) {
        const inRepo = relative(REPO_ROOT, absolute);
        if (!inRepo.startsWith('..')) node.url = `${repo}/blob/main/${inRepo.split(sep).join('/')}${hash}`;
        return;
      }

      const withoutExt = rel.replace(/\.md$/, '');
      const route = `/${withoutExt.split(sep).join('/')}`.replace(/\/index$/, '');
      node.url = `${prefix}${route || '/'}${hash}`;
    };

    const walk = (node) => {
      if (node.type === 'link' || node.type === 'definition') rewrite(node);
      for (const child of node.children ?? []) walk(child);
    };
    walk(tree);
  };
}
