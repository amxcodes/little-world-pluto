import { build } from 'esbuild';
import { readFile, writeFile, mkdir } from 'node:fs/promises';

await mkdir('dist', { recursive: true });
const result = await build({
  entryPoints: ['src/main.jsx'],
  bundle: true,
  minify: true,
  platform: 'browser',
  format: 'iife',
  target: 'es2022',
  write: false,
});
const css = (await Promise.all(['src/style.css', 'src/conversion.css', 'src/settings.css', 'src/glass.css', 'src/meetings.css'].map(path => readFile(path, 'utf8')))).join('\n');
const notices = (await readFile('THIRD_PARTY_NOTICES.md', 'utf8')).replaceAll('--', '— —').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
const js = result.outputFiles[0].text.replaceAll('</script', '<\\/script');
const html = `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><style>${css}</style></head><body><div id="root"></div><script>${js}</script><!-- THIRD-PARTY NOTICES\n${notices}\n-->\n</body></html>`;
await writeFile('dist/index.html', html);
console.log(`Built Pluto React UI (${Buffer.byteLength(html)} bytes)`);
