// Disposable native CI: actual packaged extension -> Chromium -> installed native host -> bridge.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {chromium} = require(path.join(process.env.RUNNER_TEMP, 'saya-browser-test/node_modules/playwright'));
assert.equal(process.env.GITHUB_ACTIONS, 'true');
const [extension, tokenFile, work] = process.argv.slice(2);
const expected = fs.readFileSync(tokenFile, 'utf8').trim();
assert.ok(expected);
const host = 'com.saya.studio';
const extensionId = 'hgbfcjoeaddihdpccfhcgeohohfejidm';
async function launch(folder, name) {
  return chromium.launchPersistentContext(path.join(work, name), {
    channel: 'chromium', headless: true,
    args: [`--disable-extensions-except=${folder}`, `--load-extension=${folder}`],
  });
}
async function worker(context) {
  return context.serviceWorkers()[0] || await context.waitForEvent('serviceworker', {timeout: 30000});
}
(async () => {
  const context = await launch(extension, 'browser-profile');
  try {
    const sw = await worker(context);
    assert.equal(new URL(sw.url()).host, extensionId);
    const result = await sw.evaluate(async ({host, expected}) => {
      const native = await new Promise(resolve => chrome.runtime.sendNativeMessage(host, {type:'get_token'}, reply => {
        resolve({error:chrome.runtime.lastError?.message, matched:reply?.token === expected, ok:reply?.ok});
      }));
      if (native.error || !native.ok || !native.matched) return {native};
      const jobs = await bridge('/jobs');
      return {native, bridge: Array.isArray(jobs.jobs), source: tokenSource};
    }, {host, expected});
    assert.equal(result.native.error, undefined, JSON.stringify(result));
    assert.equal(result.native.matched, true);
    assert.equal(result.bridge, true);
    assert.equal(result.source, 'native');
  } finally { await context.close(); }
  const denied = path.join(work, 'untrusted-extension');
  fs.mkdirSync(denied);
  fs.writeFileSync(path.join(denied, 'manifest.json'), JSON.stringify({manifest_version:3,name:'Native host refusal test',version:'1.0',permissions:['nativeMessaging'],background:{service_worker:'background.js'}}));
  fs.writeFileSync(path.join(denied, 'background.js'), 'chrome.runtime.onInstalled.addListener(() => {});');
  const other = await launch(denied, 'untrusted-profile');
  try {
    const sw = await worker(other);
    assert.notEqual(new URL(sw.url()).host, extensionId);
    const result = await sw.evaluate(host => new Promise(resolve => chrome.runtime.sendNativeMessage(host, {type:'get_token'}, reply => {
      resolve({refused:!!chrome.runtime.lastError, leaked:typeof reply?.token === 'string'});
    })), host);
    assert.equal(result.refused, true);
    assert.equal(result.leaked, false);
  } finally { await other.close(); }
  console.log('Native browser integration passed: packaged extension, real host, authenticated bridge, other extension refused');
})().catch(error => {console.error(error);process.exitCode=1;});
