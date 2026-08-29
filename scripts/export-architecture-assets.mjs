import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  ChromeVisualBrowser,
  findChrome,
} from '../.agents/skills/archify/bin/visual-check.mjs';

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const repositoryRoot = path.resolve(scriptDirectory, '..');
const architectureDirectory = path.join(repositoryRoot, 'docs', 'architecture');
const artifactPath = path.join(architectureDirectory, 'disk-project-organizer.html');
const outputStem = 'disk-project-organizer-architecture';

function sha256(filePath) {
  return createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

async function waitForDownload(filePath, timeoutMs = 30_000) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    if (fs.existsSync(filePath) && fs.statSync(filePath).size > 0) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Timed out waiting for architecture export: ${filePath}`);
}

async function evaluate(browser, sessionId, expression) {
  const response = await browser.cdp.send(
    'Runtime.evaluate',
    { expression, awaitPromise: true, returnByValue: true },
    sessionId,
    60_000,
  );
  if (response.exceptionDetails) {
    throw new Error(
      response.exceptionDetails.exception?.description
        || response.exceptionDetails.text
        || 'Architecture export failed in Chrome.',
    );
  }
  return response.result?.value;
}

const chromePath = findChrome();
if (!chromePath) {
  throw new Error('Chrome is required. Set ARCHIFY_CHROME to the Chrome executable.');
}
if (!fs.existsSync(artifactPath)) {
  throw new Error(`Delivered architecture artifact not found: ${artifactPath}`);
}

const downloadDirectory = fs.mkdtempSync(path.join(os.tmpdir(), 'archify-export-'));
const browser = new ChromeVisualBrowser(chromePath);

try {
  await browser.inspect({
    artifactPath,
    width: 1600,
    height: 1000,
    theme: 'light',
  });
  const sessionId = await browser.sessionPromise;

  try {
    await browser.cdp.send('Browser.setDownloadBehavior', {
      behavior: 'allow',
      downloadPath: downloadDirectory,
      eventsEnabled: true,
    });
  } catch {
    await browser.cdp.send(
      'Page.setDownloadBehavior',
      { behavior: 'allow', downloadPath: downloadDirectory },
      sessionId,
    );
  }

  const exports = [];
  for (const format of ['svg', 'png']) {
    const downloadedPath = path.join(downloadDirectory, `${outputStem}.${format}`);
    const destinationPath = path.join(architectureDirectory, `${outputStem}.${format}`);
    const receipt = await evaluate(browser, sessionId, `(async function () {
      document.title = '${outputStem}';
      await Archify.exportMenu.run('${format}');
      return {
        format: document.documentElement.getAttribute('data-last-export-format'),
        bytes: Number(document.documentElement.getAttribute('data-last-export-bytes')),
        canonical: document.documentElement.getAttribute('data-last-export-canonical') === 'true',
        error: document.documentElement.getAttribute('data-last-export-error')
      };
    })()`);

    if (receipt?.error || receipt?.format !== format || !receipt?.canonical) {
      throw new Error(`Archify ${format} export returned an invalid receipt: ${JSON.stringify(receipt)}`);
    }

    await waitForDownload(downloadedPath);
    fs.copyFileSync(downloadedPath, destinationPath);
    const bytes = fs.statSync(destinationPath).size;
    if (bytes !== receipt.bytes) {
      throw new Error(`Archify ${format} byte receipt mismatch: expected ${receipt.bytes}, got ${bytes}.`);
    }
    exports.push({
      format,
      path: path.relative(repositoryRoot, destinationPath).replaceAll('\\', '/'),
      bytes,
      sha256: sha256(destinationPath),
      canonical: receipt.canonical,
    });
  }

  process.stdout.write(`${JSON.stringify({ ok: true, source: path.relative(repositoryRoot, artifactPath), exports }, null, 2)}\n`);
} finally {
  await browser.close();
  fs.rmSync(downloadDirectory, { recursive: true, force: true });
}
