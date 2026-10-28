// Optional real-browser regression checks. Uses an existing Playwright install;
// set PLUTO_PLAYWRIGHT_PATH and PLUTO_BROWSER_PATH when not on Node's search path.
import assert from 'node:assert/strict';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { mkdir } from 'node:fs/promises';

const { chromium } = await import(process.env.PLUTO_PLAYWRIGHT_PATH
  ? pathToFileURL(process.env.PLUTO_PLAYWRIGHT_PATH).href : 'playwright');
const browser = await chromium.launch({ headless: true, ...(process.env.PLUTO_BROWSER_PATH ? { executablePath: process.env.PLUTO_BROWSER_PATH } : {}) });
const output = new URL('../../target/ui-review/', import.meta.url);
await mkdir(output, { recursive: true });
let checks = 0;
const pageErrors = [];
const rgba = value => (value.match(/[\d.]+/g) || []).map(Number);
const luminance = rgb => rgb.slice(0, 3).map(n => n / 255).map(n => n <= .04045 ? n / 12.92 : ((n + .055) / 1.055) ** 2.4).reduce((sum, n, i) => sum + n * [.2126, .7152, .0722][i], 0);
const contrast = (a, b) => (Math.max(luminance(a), luminance(b)) + .05) / (Math.min(luminance(a), luminance(b)) + .05);
try {
  for (const scale of [1, 1.25, 2]) for (const glass of [false, true]) for (const dark of [false, true]) {
    const context = await browser.newContext({ viewport: { width: 680, height: 448 }, deviceScaleFactor: scale });
    const page = await context.newPage();
    page.on('pageerror', error => pageErrors.push(error.message));
    await page.addInitScript(({ glass }) => {
      localStorage.setItem('pluto.material.v1', JSON.stringify({ enabled: glass, motion: true }));
      localStorage.setItem('island.agent-preferences', JSON.stringify({ providerSetupComplete: true }));
      window.ipc = { postMessage(raw) {
        const message = JSON.parse(raw);
        // Reproduce only native page acknowledgement; never execute OS actions.
        if (message.kind === 'page') window.islandReceive?.({ page: message.page });
      } };
    }, { glass });
    await page.goto(new URL('../dist/index.html', import.meta.url).href);
    if (glass) await page.evaluate(dark => { document.body.style.background = dark ? '#000' : '#f6f6f7'; }, dark);
    await page.waitForFunction(() => typeof window.islandReceive === 'function');
    await page.evaluate(({ dark }) => {
      window.islandReceive({ expanded: true, dark, accentGradient: 'mint', page: 'Home', agents: { opencode: { connected: true, sessions: [], models: [] } } });
      window.islandOpenSettings();
    }, { dark });
    await page.locator('.settings-topbar').waitFor();
    if (glass) {
      assert.equal(await page.locator('.island').evaluate(el => getComputedStyle(el).getPropertyValue('--bg').trim()), dark ? '#000' : '#f6f6f7', 'glass shell follows the stored theme with a solid backing');
      checks++;
      assert.equal(await page.locator('.settings-segments').first().evaluate(el => getComputedStyle(el).backdropFilter), 'none', 'segmented containers must not acquire a dark refracted backing'); checks++;
      const rail = await page.locator('.settings-sidebar').evaluate(el => ({ top: el.getBoundingClientRect().top, first: el.firstElementChild.getBoundingClientRect().top }));
      assert.ok(rail.first - rail.top >= 5, 'sidebar lenses need paint room above their top edge'); checks++;
    }
    const header = async () => {
      const result = await page.locator('.bar').evaluate(el => {
        const r = el.getBoundingClientRect();
        return { y: r.y, scroll: window.scrollY, children: [...el.children].map(child => { const c = child.getBoundingClientRect(); return { y: c.y, bottom: c.bottom }; }), bottom: r.bottom };
      });
      assert.equal(result.scroll, 0, 'focus must not scroll the document');
      assert.equal(result.y, 0, 'toolbar must stay at the viewport origin');
      for (const child of result.children) assert.ok(child.y >= (glass ? 4 : 0) && child.bottom <= result.bottom, 'toolbar child needs room for its glass rim');
      checks++;
    };
    for (const section of ['General', 'Status', 'Priority', 'Motion', 'Tools', 'Quick actions', 'Agents', 'Dictation', 'Voice', 'Ask', 'Conversion']) {
      await page.locator('.settings-sidebar').getByRole('button', { name: section, exact: true }).click();
      await page.locator('.settings-content').evaluate(el => {
        const last = [...el.querySelectorAll('button:not(:disabled),input:not(:disabled),textarea')].at(-1);
        last?.focus(); // Deliberately omit preventScroll: the page must handle it.
      });
      await header();
      if (glass && section === 'Dictation') {
        const lens = await page.locator('.settings-provider-list button').first().evaluate(el => getComputedStyle(el).backgroundImage);
        assert.ok(lens.includes('radial-gradient'), 'provider buttons use the shared glass lens'); checks++;
      }
    }
    await page.locator('.settings-sidebar').getByRole('button', { name: 'General', exact: true }).click();
    for (const accent of ['lime', 'ice', 'rose', 'violet', 'amber', 'peach', 'mint']) {
      await page.evaluate(accentGradient => window.islandReceive({ accentGradient }), accent);
      await page.waitForFunction(accent => document.querySelector('.island').classList.contains(`accent-${accent}`), accent);
      const selected = await page.locator('.settings-segments button.selected').first().evaluate(el => ({ color: getComputedStyle(el).color, accent: getComputedStyle(el).getPropertyValue('--accent-b') }));
      const hex = selected.accent.trim().slice(1).match(/../g).map(n => parseInt(n, 16));
      assert.ok(contrast(rgba(selected.color), hex) >= 4.5, `${accent} selected text must contrast against its accent fill`); checks++;
    }
    if (glass && scale === 1.25) {
      await page.locator('.settings-content').evaluate(el => { el.scrollTop = 0; });
      await page.screenshot({ path: fileURLToPath(new URL(`settings-${dark ? 'dark' : 'light'}-verified.png`, output)) });
    }
    await page.getByRole('button', { name: 'Back to app', exact: true }).click();
    await page.locator('.settings-topbar').waitFor({ state: 'detached' });
    for (const name of ['Home', 'Clipboard', 'Shelf', 'Text', 'Agents', 'Ask', 'Note', 'Weather', 'Controls']) {
      await page.locator('nav[aria-label="Island pages"]').getByRole('button', { name, exact: true }).click();
      await page.waitForFunction(name => document.querySelector('.tab.active')?.getAttribute('aria-label') === name, name);
      await header();
      if (glass && name === 'Home') {
        const tools = page.getByRole('region', { name: 'Quick actions', exact: true });
        const spacing = await tools.evaluate(el => ({ gap: parseFloat(getComputedStyle(el).gap), labels: [...el.querySelectorAll('button span')].map(label => ({ text: label.textContent, fits: label.scrollWidth <= label.clientWidth })) }));
        assert.ok(spacing.gap >= 6, 'quick actions need a visible gap'); checks++;
        for (const label of spacing.labels) { assert.ok(label.fits, label.text + ' quick action label must fit'); checks++; }
        const capture = tools.getByRole('button', { name: 'Capture', exact: true });
        await capture.hover();
        const icon = await capture.locator('svg').evaluate(el => ({ background: getComputedStyle(el).backgroundColor, transform: getComputedStyle(el).transform }));
        assert.equal(icon.background, 'rgba(0, 0, 0, 0)', 'utility hover keeps the icon clear');
        assert.equal(icon.transform, 'none', 'utility hover must not use the old icon zoom'); checks += 2;
        if (scale === 1.25) await page.screenshot({ path: fileURLToPath(new URL('utilities-' + (dark ? 'dark' : 'light') + '.png', output)) });
      }
      if (glass) {
        const navigation = await page.locator('.tab-indicator').evaluate(el => {
          // CSS color-mix serializes as color(srgb ...), not always rgb(0..255).
          const canvas = document.createElement('canvas'); canvas.width = canvas.height = 1;
          const ctx = canvas.getContext('2d'); ctx.fillStyle = getComputedStyle(el).backgroundColor; ctx.fillRect(0, 0, 1, 1);
          const pixel = [...ctx.getImageData(0, 0, 1, 1).data]; pixel[3] /= 255;
          return { background: pixel, color: getComputedStyle(el.parentElement.querySelector('.tab.active')).color };
        });
        const fill = navigation.background;
        const backdrop = dark ? 0 : 246; // Match the solid native shell.
        const composed = fill.slice(0, 3).map(channel => channel * (fill[3] ?? 1) + backdrop * (1 - (fill[3] ?? 1)));
        assert.ok(contrast(rgba(navigation.color), composed) >= 4.5, 'navigation label needs contrast over the themed shell'); checks++;
        const nested = await page.locator('.clipboard-search input,.wx-search input,.agent-composer textarea').evaluateAll(elements => elements.map(el => getComputedStyle(el).backgroundColor));
        if (['Clipboard', 'Weather', 'Agents'].includes(name)) assert.ok(nested.length > 0, `${name} field must actually be exercised`);
        for (const color of nested) assert.equal(color, 'rgba(0, 0, 0, 0)', 'nested editor paints a second surface');
        checks += nested.length;
      }
      if (name === 'Ask' && glass) {
        const inset = await page.locator('.voice-answer-text').evaluate(el => parseFloat(getComputedStyle(el).paddingLeft));
        assert.ok(inset >= 16, 'Ask response needs a readable inset'); checks++;
      }
      if (name === 'Controls') {
        await page.evaluate(() => window.islandReceive({ radios: { wifi: true, bluetooth: false } }));
        const tile = await page.locator('.control-tile.control-active strong').first().evaluate(el => ({ color: getComputedStyle(el).color, expected: getComputedStyle(el.closest('.island')).color }));
        assert.equal(tile.color, tile.expected, 'active tile title must retain readable text color'); checks++;
      }
      if (glass && dark && scale === 1.25 && ['Clipboard', 'Ask', 'Agents'].includes(name)) await page.screenshot({ path: fileURLToPath(new URL(`${name.toLowerCase()}-verified.png`, output)) });
    }
    if (glass) {
      const current = page.locator('.tab.active');
      await current.hover();
      await page.mouse.down();
      assert.equal(await current.evaluate(el => getComputedStyle(el).boxShadow), 'none', 'pointer press must not add a white halo');
      await page.mouse.up(); checks++;
      await page.evaluate(() => window.islandOpenSettings());
      await page.locator('.settings-sidebar').getByRole('button', { name: 'General', exact: true }).click();
      const action = page.locator('.settings-inline-action').first();
      const motion = await action.evaluate(el => ({ transition: getComputedStyle(el).transitionProperty, flags: el.closest('.island').outerHTML.slice(0, 550), reduced: matchMedia('(prefers-reduced-motion: reduce)').matches }));
      assert.ok(motion.transition.includes('translate'), `glass buttons ease their hover lift: ${JSON.stringify(motion)}`); checks++;
      await page.emulateMedia({ reducedMotion: 'reduce' });
      await action.hover();
      assert.equal(await action.evaluate(el => getComputedStyle(el).transitionDuration), '0s', 'reduced motion stops button transitions'); checks++;
      assert.equal(await action.evaluate(el => getComputedStyle(el).translate), 'none', 'reduced motion stops hover lift'); checks++;
      await page.emulateMedia({ reducedMotion: 'no-preference' });
      await page.getByRole('switch', { name: 'Glass motion', exact: true }).click();
      assert.equal(await page.locator('.island').getAttribute('data-glass-motion'), 'false');
      const animation = await page.locator('.page-content').evaluate(el => getComputedStyle(el).animationName);
      assert.equal(animation, 'none', 'Glass motion off must stop page motion'); checks++;
    }
    await context.close();
  }
  assert.deepEqual(pageErrors, [], 'runtime exceptions');
  console.log(`${checks} browser checks passed: light/dark, glass/standard, 100/125/200% scale, 11 settings sections and 9 app pages.`);
} finally { await browser.close(); }
