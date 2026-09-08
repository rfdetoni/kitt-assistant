import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assets = new URL('../apps/kittd/control-center-web/', import.meta.url);

test('control center: layout, editing focus, pending changes, review and mobile', async () => {
  const browser = await chromium.launch({headless: true});
  try {
    const page = await browser.newPage({viewport: {width: 1440, height: 900}, reducedMotion: 'reduce'});
    const errors = [], writes = [];
    page.on('pageerror', error => errors.push(error.message));
    const catalog = JSON.parse(await readFile(new URL('catalog.json', assets), 'utf8'));
    const snapshot = {revision: 1, values: Object.fromEntries(catalog.sections.map(s => [s.id, Object.fromEntries(s.fields.map(f => [f.key, f.default ?? null]))]))};
    await page.route('http://kitt.test/**', async route => {
      const path = new URL(route.request().url()).pathname;
      if (['/', '/app.js', '/app.css'].includes(path)) {
        return route.fulfill({body: await readFile(new URL(path === '/' ? 'index.html' : path.slice(1), assets)), contentType: path.endsWith('.js') ? 'text/javascript' : path.endsWith('.css') ? 'text/css' : 'text/html'});
      }
      let payload = path.endsWith('/catalog') ? catalog : path.endsWith('/config') ? snapshot
        : path.endsWith('/health') ? {status: 'ok', csrf_token: 'fixture'}
        : path.endsWith('/service/status') ? {daemon: {active: true, pid: 42}, voice: {}, models: {}, memory: {}}
        : {};
      if (route.request().method() === 'PUT') {
        const body = route.request().postDataJSON(); writes.push(body);
        for (const [section, changes] of Object.entries(body.changes)) Object.assign(snapshot.values[section], changes);
        snapshot.revision++; payload = {snapshot};
      }
      if (path.endsWith('/validate')) payload = {diff: route.request().postDataJSON().changes};
      await route.fulfill({json: payload});
    });
    await page.goto('http://kitt.test/');
    await page.locator('[data-nav="agent.remote"]').click();
    const handle = page.locator('#sidebar-resize');
    await handle.focus(); await page.keyboard.press('ArrowRight');
    assert.equal(await handle.getAttribute('aria-valuenow'), '270');
    await page.reload();
    assert.equal(await handle.getAttribute('aria-valuenow'), '270');
    await page.locator('[data-nav="agent.remote"]').click();
    const host = page.locator('[data-key="agent.remote::host"]');
    const original = await host.inputValue();
    await host.fill('localhost');
    await page.locator('[data-key="agent.remote::port"]').click();
    assert.equal(await page.locator('[data-key="agent.remote::port"]').evaluate(el => el === document.activeElement), true);
    assert.match(await page.locator('#pending-status').textContent(), /1 alteração pendente/);
    await host.fill(original);
    assert.equal(await page.locator('#apply-all').isDisabled(), true);
    await host.fill('localhost');
    await page.locator('#apply-all').click();
    await page.locator('#diff-dialog').waitFor({state: 'visible'});
    assert.equal(await page.locator('#diff-dialog').isVisible(), true);
    assert.equal(writes.length, 0);
    await page.locator('#confirm-apply').click();
    await page.waitForFunction(() => document.getElementById('pending-status').textContent === 'Sem alterações pendentes');
    assert.equal(writes.length, 1);
    assert.deepEqual(writes[0].changes, {'agent.remote': {host: 'localhost'}});
    await page.keyboard.press('Control+k');
    assert.equal(await page.locator('#search').evaluate(el => el === document.activeElement), true);
    await page.screenshot({path: '/tmp/kitt-control-center-desktop.png'});
    for (const width of [1024, 960, 768, 390, 320]) {
      await page.setViewportSize({width, height: 844});
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, `overflow at ${width}`);
    }
    await page.locator('#mobile-nav-select').selectOption('agent.remote');
    assert.equal(await host.isVisible(), true);
    await page.screenshot({path: '/tmp/kitt-control-center-mobile.png', fullPage: true});
    await page.locator('#mobile-nav-select').selectOption('__monitor__');
    await page.locator('#service-logs').waitFor();
    await page.locator('#service-logs').evaluate(el => el.style.height = '300px');
    await page.locator('#btn-refresh').click();
    await page.waitForFunction(() => document.getElementById('alerts').textContent.includes('Status atualizado'));
    assert.equal(await page.locator('#service-logs').evaluate(el => el.style.height), '300px');
    await page.setViewportSize({width: 1440, height: 900});
    await page.waitForFunction(() => document.getElementById('sidebar-resize').getAttribute('aria-valuenow') === '270');
    assert.deepEqual(errors, []);
  } finally { await browser.close(); }
});
