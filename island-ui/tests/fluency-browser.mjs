// Run with the same PLUTO_PLAYWRIGHT_PATH / PLUTO_BROWSER_PATH as glass-browser.
import assert from 'node:assert/strict';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { mkdir, writeFile } from 'node:fs/promises';
import { build } from 'esbuild';
const { chromium } = await import(process.env.PLUTO_PLAYWRIGHT_PATH ? pathToFileURL(process.env.PLUTO_PLAYWRIGHT_PATH).href : 'playwright');
const browser = await chromium.launch({ headless: true, ...(process.env.PLUTO_BROWSER_PATH ? { executablePath: process.env.PLUTO_BROWSER_PATH } : {}) });
const output = new URL('../../target/ui-review/', import.meta.url);
await mkdir(output, { recursive: true });
const report = { toolbarCases: 0, settingsCases: 0, errors: [], optics: null };
try {
  for (const scale of [1, 1.25, 2]) for (const glass of [false, true]) for (const privacy of [0, 1, 2]) {
    const context = await browser.newContext({ viewport: { width: 178, height: 28 }, deviceScaleFactor: scale });
    const page = await context.newPage();
    page.on('pageerror', error => report.errors.push(error.message));
    await page.addInitScript(({ glass }) => {
      localStorage.setItem('pluto.material.v1', JSON.stringify({ enabled: glass, motion: true }));
      window.messages = [];
      window.ipc = { postMessage(raw) { window.messages.push(JSON.parse(raw)); } };
    }, { glass });
    await page.goto(new URL('../dist/index.html', import.meta.url).href);
    await page.waitForFunction(() => window.islandReceive);
    await page.evaluate(privacy => window.islandReceive({ expanded: false, dark: true, compactFaded: false,
      clockEnabled: true, clockLabel: '12:34', microphone: privacy > 0, camera: privacy > 1,
      foregroundApp: { kind: 'browser', name: 'Browser with a long name', executable: 'msedge.exe', fullscreen: false },
      shellSettings: { openOnClick: true }, media: null, weatherData: null }), privacy);
    await page.locator('.compact-transition').hover();
    await page.waitForFunction(() => window.messages.some(m => m.kind === 'compactContext' && m.count > 0));
    const count = await page.evaluate(() => window.messages.filter(m => m.kind === 'compactContext').at(-1).count);
    const privacySpace = privacy ? privacy * 21 + (privacy - 1) * 3 + 8 : 0;
    const width = 108 + count * 27 + privacySpace;
    assert.equal(await page.locator('.compact-context-rail').getAttribute('data-ready'), 'false', 'tools wait for their native width');
    await page.setViewportSize({ width, height: 28 });
    await page.waitForFunction(() => document.querySelector('.compact-context-rail')?.dataset.ready === 'true');
    const boxes = await page.evaluate(() => {
      const bounds = el => { const r = el.getBoundingClientRect(); return { left: r.left, right: r.right, top: r.top, bottom: r.bottom }; };
      return { rail: bounds(document.querySelector('.compact-context-rail')),
        buttons: [...document.querySelectorAll('.compact-context-rail button')].map(bounds),
        privacy: [...document.querySelectorAll('.compact-privacy button')].map(bounds) };
    });
    for (const button of boxes.buttons) {
      assert.ok(button.left >= boxes.rail.left && button.right <= boxes.rail.right + .1, 'tool stays within rail');
      assert.ok(button.top >= 0 && button.bottom <= 28, 'tool stays within compact viewport');
      if (boxes.privacy.length) assert.ok(button.right + 4 <= boxes.privacy[0].left, 'privacy lane remains separate');
    }
    await page.evaluate(() => window.islandReceive({ compactFaded: true }));
    await page.waitForFunction(() => getComputedStyle(document.querySelector('.compact-context-rail')).opacity === '0');
    assert.equal(await page.locator('.compact-context-rail').evaluate(el => getComputedStyle(el).pointerEvents), 'none');
    report.toolbarCases++;
    if (glass && scale === 1.25 && privacy === 2) {
      await page.evaluate(() => window.islandReceive({ compactFaded: false }));
      await page.screenshot({ path: fileURLToPath(new URL('compact-tools-privacy.png', output)) });
    }
    await context.close();
  }
  // Start from Home's shorter native viewport, not an already-open Settings
  // page. Inspect the committed tree at the exact outgoing resize request.
  for (const glass of [false, true]) for (const dark of [false, true]) for (const reduced of [false, true]) {
    const context = await browser.newContext({ viewport: { width: 680, height: 348 } });
    const page = await context.newPage();
    page.on('pageerror', error => report.errors.push(error.message));
    await page.addInitScript(({ glass }) => {
      localStorage.setItem('pluto.material.v1', JSON.stringify({ enabled: glass, motion: true }));
      window.settingsLayouts = [];
      window.ipc = { postMessage(raw) {
        const message = JSON.parse(raw);
        if (message.kind !== 'layout' || !message.settingsOpen) return;
        const root = document.querySelector('.island');
        const content = document.querySelector('.settings-page');
        const header = document.querySelector('.settings-topbar');
        window.settingsLayouts.push({
          height: root.getBoundingClientRect().height,
          content: !!content, home: !!document.querySelector('.home-layout'),
          headerTop: header.getBoundingClientRect().top, scroll: window.scrollY,
          pageAnimation: getComputedStyle(document.querySelector('.page-content')).animationName,
        });
      } };
    }, { glass });
    await page.goto(new URL('../dist/index.html', import.meta.url).href);
    await page.waitForFunction(() => window.islandReceive);
    await page.evaluate(({ dark, reduced }) => window.islandReceive({ expanded: true, dark, page: 'Home',
      compactRenderGeneration: 7, shellSettings: { reducedMotion: reduced } }), { dark, reduced });
    const cdp = await context.newCDPSession(page);
    await cdp.send('Emulation.setCPUThrottlingRate', { rate: 4 });
    await page.getByRole('button', { name: 'Island settings', exact: true }).click();
    // Live page data must not restart or erase the Settings first-paint work.
    await page.evaluate(async () => {
      for (let i = 0; i < 8; i++) {
        window.islandReceive({ weatherData: { hourly: { time: [] } }, noteFolders: Array(i).fill('Folder') });
        await new Promise(resolve => requestAnimationFrame(resolve));
      }
    });
    await page.waitForFunction(() => window.settingsLayouts.length > 0);
    const layouts = await page.evaluate(() => window.settingsLayouts);
    assert.equal(layouts.length, 1, 'background data must not restart Settings layout');
    assert.deepEqual({ ...layouts[0], headerTop: 0 }, {
      height: 448, content: true, home: false, headerTop: 0, scroll: 0, pageAnimation: 'none',
    });
    assert.ok(layouts[0].headerTop >= 0, 'first Settings toolbar stays inside the viewport');
    await page.setViewportSize({ width: 680, height: 448 });
    assert.equal(await page.locator('.settings-topbar').evaluate(el => el.getBoundingClientRect().top), layouts[0].headerTop,
      'native growth must not move the Settings toolbar');
    await page.locator('.settings-content').evaluate(el => { el.scrollTop = 100; });
    await page.evaluate(() => window.islandRequestLayout());
    await page.waitForFunction(() => window.settingsLayouts.length === 2);
    assert.ok(await page.locator('.settings-content').evaluate(el => el.scrollTop > 0), 'native layout requests retain reading position');
    report.settingsCases++;
    await context.close();
  }
  const page = await browser.newPage();
  await page.setContent('<div id="surface"><svg><defs></defs></svg><div class="settings-card" style="width:240px;height:80px;border-radius:18px"><span id="stream"></span></div></div>');
  const bundle = await build({ entryPoints: [fileURLToPath(new URL('../src/glass-optics.js', import.meta.url))], bundle: true, write: false, format: 'iife', globalName: 'Optics' });
  await page.addScriptTag({ content: bundle.outputFiles[0].text });
  report.optics = await page.evaluate(async () => {
    const root = document.querySelector('#surface'), defs = root.querySelector('defs');
    const frame = () => new Promise(resolve => requestAnimationFrame(resolve));
    let rootScans = 0;
    const query = root.querySelectorAll.bind(root);
    root.querySelectorAll = (...args) => { rootScans++; return query(...args); };
    const cleanup = Optics.installGlassOptics(root, defs, 5);
    for (let i = 0; i < 100; i++) { document.querySelector('#stream').textContent = `Streaming ${i}`; await frame(); }
    const scansAfterStreaming = rootScans;
    const added = document.createElement('div'); added.className = 'settings-card'; added.style.cssText = 'width:100px;height:40px;border-radius:12px';
    root.append(added); await frame(); await frame();
    const afterAdd = defs.children.length;
    added.remove(); await frame(); await frame();
    const afterRemove = defs.children.length;
    const pending = document.createElement('div'); pending.className = 'settings-card'; root.append(pending);
    await Promise.resolve(); cleanup(); await frame(); await frame();
    const afterCleanup = defs.children.length;
    const zeroCleanup = Optics.installGlassOptics(root, defs, 0);
    const atZero = defs.children.length; zeroCleanup();
    return { scansAfterStreaming, afterAdd, afterRemove, afterCleanup, atZero };
  });
  assert.deepEqual(report.optics, { scansAfterStreaming: 1, afterAdd: 2, afterRemove: 1, afterCleanup: 0, atZero: 0 });
  assert.deepEqual(report.errors, []);
  await writeFile(new URL('fluency-results.json', output), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report));
} finally { await browser.close(); }
