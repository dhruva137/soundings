#!/usr/bin/env node
// soundings-app: a tiny, dependency-free helper.
//   npx soundings-app            download the latest Soundings installer from GitHub, check its SHA-256, run it
//   npx soundings-app mcp        run the installed Soundings as a read-only MCP server (for Claude)
//   npx soundings-app open       open the installed app
'use strict';
const { spawn } = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const REPO = 'dhruva137/soundings';
const EXE = path.join(process.env.LOCALAPPDATA || path.join(os.homedir(), 'AppData', 'Local'), 'Soundings', 'soundings.exe');

function fail(msg) { process.stderr.write(`soundings-app: ${msg}\n`); process.exit(1); }

async function get(url) {
  const r = await fetch(url, { headers: { 'User-Agent': 'soundings-app', Accept: 'application/vnd.github+json' } });
  if (!r.ok) fail(`${url} returned ${r.status}`);
  return r;
}

async function install(silent) {
  if (process.platform !== 'win32') fail('Soundings is a Windows app.');
  const rel = await (await get(`https://api.github.com/repos/${REPO}/releases/latest`)).json();
  const setup = rel.assets.find((a) => /_x64-setup\.exe$/i.test(a.name));
  const sums = rel.assets.find((a) => /^SHA256SUMS/i.test(a.name));
  if (!setup || !sums) fail(`release ${rel.tag_name} is missing the installer or SHA256SUMS.txt`);
  const want = (await (await get(sums.browser_download_url)).text()).split(/\r?\n/)
    .map((l) => l.trim().split(/\s+/)).find(([, n]) => n === setup.name)?.[0]?.toLowerCase();
  if (!want) fail(`no checksum listed for ${setup.name}`);
  console.log(`Downloading ${setup.name} (${(setup.size / 1048576).toFixed(1)} MB)...`);
  const buf = Buffer.from(await (await get(setup.browser_download_url)).arrayBuffer());
  const got = crypto.createHash('sha256').update(buf).digest('hex');
  if (got !== want) fail(`checksum mismatch (expected ${want}, got ${got}); not running it.`);
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'soundings-'));
  const file = path.join(dir, setup.name);
  fs.writeFileSync(file, buf);
  console.log('Checksum OK. Starting the installer...');
  const p = spawn(file, silent ? ['/S'] : [], { stdio: 'inherit' });
  p.on('exit', (code) => {
    fs.rmSync(dir, { recursive: true, force: true });
    if (code === 0) console.log(`Soundings ${rel.tag_name} is installed. Find it in the Start menu or on your Desktop.`);
    process.exit(code ?? 0);
  });
}

function needExe() {
  if (!fs.existsSync(EXE)) fail('Soundings is not installed yet. Run `npx soundings-app` first.');
}

const cmd = process.argv[2] || 'install';
if (cmd === 'install') install(process.argv.includes('--silent')).catch((e) => fail(e.message));
else if (cmd === 'mcp') { needExe(); spawn(EXE, ['--mcp'], { stdio: 'inherit', windowsHide: true }).on('exit', (c) => process.exit(c ?? 0)); }
else if (cmd === 'open') { needExe(); spawn(EXE, [], { detached: true, stdio: 'ignore' }).unref(); }
else {
  console.log(`Usage:
  npx soundings-app [install] [--silent]   install the latest Soundings (SHA-256 checked)
  npx soundings-app open                    open Soundings
  npx soundings-app mcp                     run Soundings as a read-only MCP server for Claude`);
}
