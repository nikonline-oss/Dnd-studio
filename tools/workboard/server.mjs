#!/usr/bin/env node
// Workboard: a local task board and repository viewer for this project.
//
//   node tools/workboard/server.mjs            → http://127.0.0.1:8090
//   WORKBOARD_PORT=9000 node tools/workboard/server.mjs
//   WORKBOARD_HOST=0.0.0.0 WORKBOARD_TOKEN=secret node tools/workboard/server.mjs
//
// No dependencies: Node's own http/fs are enough, so the tool builds nothing and works offline.
// It is a development aid, not part of the DndStudio product (CLAUDE.md §12): it only reads the
// repository and writes `docs/workboard/tasks.json`.

import { createServer } from 'node:http';
import { readFile, writeFile, rename, stat, readdir } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { execFile } from 'node:child_process';
import { extname, join, normalize, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { randomBytes, timingSafeEqual } from 'node:crypto';

const run = promisify(execFile);
const HERE = resolve(fileURLToPath(new URL('.', import.meta.url)));
const ROOT = resolve(HERE, '../..');
const PUBLIC = join(HERE, 'public');
const TASKS = join(ROOT, 'docs/workboard/tasks.json');

const PORT = Number(process.env.WORKBOARD_PORT ?? 8090);
const HOST = process.env.WORKBOARD_HOST ?? '127.0.0.1';
const TOKEN = process.env.WORKBOARD_TOKEN ?? '';

/** Parts of the repository the viewer never serves: build output, dependencies, git internals and
 *  secrets. Everything else is project source and documentation, which is what this tool is for. */
const DENIED = [
  /(^|\/)\.git(\/|$)/,
  /(^|\/)node_modules(\/|$)/,
  /(^|\/)target(\/|$)/,
  /(^|\/)\.env($|\.local$|\.production$)/,
  /\.(pem|key|pkcs8|p12)$/i,
  /(^|\/)id_(rsa|ed25519)/,
];

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.md': 'text/markdown; charset=utf-8',
  '.png': 'image/png',
};

function json(res, status, body) {
  const text = JSON.stringify(body);
  res.writeHead(status, {
    'content-type': 'application/json; charset=utf-8',
    'cache-control': 'no-store',
    'content-length': Buffer.byteLength(text),
  });
  res.end(text);
}

function fail(res, status, message) {
  json(res, status, { error: message });
}

/** Constant-time token comparison; an unset token means "local use, no password". */
function authorised(req, url) {
  if (!TOKEN) return true;
  const header = req.headers.authorization ?? '';
  const given = header.startsWith('Bearer ')
    ? header.slice(7)
    : (url.searchParams.get('token') ?? '');
  const a = Buffer.from(given);
  const b = Buffer.from(TOKEN);
  return a.length === b.length && timingSafeEqual(a, b);
}

/** Resolves a repository-relative path, refusing escapes and the denied areas above. */
function safePath(rel) {
  if (!rel || rel.includes('\0')) return null;
  const cleaned = normalize(rel).replaceAll('\\', '/');
  if (cleaned.startsWith('..') || cleaned.startsWith('/')) return null;
  if (DENIED.some((pattern) => pattern.test(cleaned))) return null;
  const full = resolve(ROOT, cleaned);
  if (full !== ROOT && !full.startsWith(ROOT + sep)) return null;
  return full;
}

async function readBody(req, limit = 1024 * 1024) {
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > limit) throw new Error('body too large');
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString('utf8');
}

// ---------- tasks ----------

const EMPTY = { version: 1, columns: [], tasks: [] };

const COLUMNS = [
  { id: 'backlog', title: 'Бэклог', hint: 'Не начато' },
  { id: 'doing', title: 'В работе', hint: 'Пишу код' },
  { id: 'verify', title: 'Нужно проверить', hint: 'Ждёт вашей проверки' },
  { id: 'done', title: 'Готово', hint: 'Проверено' },
];

async function loadTasks() {
  if (!existsSync(TASKS)) return structuredClone(EMPTY);
  const text = await readFile(TASKS, 'utf8');
  const data = JSON.parse(text);
  return { version: data.version ?? 1, columns: COLUMNS, tasks: data.tasks ?? [] };
}

/** Write via a temporary file: a crash must not leave half a board behind. */
async function saveTasks(data) {
  const payload = JSON.stringify({ version: 1, tasks: data.tasks }, null, 2) + '\n';
  const tmp = `${TASKS}.tmp`;
  await writeFile(tmp, payload);
  await rename(tmp, TASKS);
}

function cleanTask(input, existing) {
  const title = String(input.title ?? existing?.title ?? '').trim();
  if (!title) throw new Error('title is required');
  const column = String(input.column ?? existing?.column ?? 'backlog');
  if (!COLUMNS.some((c) => c.id === column)) throw new Error('unknown column');
  return {
    id: existing?.id ?? input.id ?? `t-${randomBytes(4).toString('hex')}`,
    title: title.slice(0, 200),
    description: String(input.description ?? existing?.description ?? '').slice(0, 8000),
    column,
    priority: ['low', 'normal', 'high'].includes(input.priority ?? existing?.priority)
      ? (input.priority ?? existing?.priority)
      : 'normal',
    kind: ['task', 'bug', 'question'].includes(input.kind ?? existing?.kind)
      ? (input.kind ?? existing?.kind)
      : 'task',
    files: (input.files ?? existing?.files ?? []).slice(0, 50).map((f) => String(f).slice(0, 300)),
    checks: (input.checks ?? existing?.checks ?? []).slice(0, 100).map((check) => ({
      label: String(check.label ?? '').slice(0, 300),
      // pass | fail | pending — a check is a claim about the real system, not a todo.
      state: ['pass', 'fail', 'pending'].includes(check.state) ? check.state : 'pending',
      note: String(check.note ?? '').slice(0, 1000),
    })),
    createdAt: existing?.createdAt ?? new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  };
}

// ---------- repository ----------

async function tree(rel) {
  const full = safePath(rel || 'docs');
  if (!full) throw new Error('outside the viewable area');
  const info = await stat(full);
  if (info.isFile())
    return [{ name: relative(ROOT, full), path: relative(ROOT, full), type: 'file' }];
  const entries = await readdir(full, { withFileTypes: true });
  const nodes = [];
  for (const entry of entries.sort((a, b) => a.name.localeCompare(b.name))) {
    // Skip build output, dependencies and git internals; a `.env.example`-style file is fine.
    if (['.git', 'node_modules', 'target'].includes(entry.name)) continue;
    if (entry.name === '.env') continue;
    const childRel = relative(ROOT, join(full, entry.name));
    if (entry.isDirectory()) {
      nodes.push({ name: entry.name, path: childRel, type: 'dir', children: await tree(childRel) });
    } else {
      nodes.push({
        name: entry.name,
        path: childRel,
        type: 'file',
        size: (await stat(join(full, entry.name))).size,
      });
    }
  }
  return nodes;
}

async function git(args) {
  try {
    const { stdout } = await run('git', args, { cwd: ROOT, maxBuffer: 8 * 1024 * 1024 });
    return stdout;
  } catch (error) {
    return String(error.stdout ?? error.message);
  }
}

async function changes() {
  const status = await git(['status', '--porcelain=v1', '-uall']);
  const stat = await git(['diff', '--stat', 'HEAD']);
  const files = status
    .split('\n')
    .filter(Boolean)
    .map((line) => ({
      code: line.slice(0, 2).trim(),
      path: line.slice(3).trim(),
      untracked: line.startsWith('??'),
    }));
  const branch = (await git(['rev-parse', '--abbrev-ref', 'HEAD'])).trim();
  const head = (await git(['log', '-1', '--oneline'])).trim();
  return { branch, head, files, stat: stat.trim() };
}

// ---------- http ----------

async function route(req, res, url) {
  const path = url.pathname;

  if (path === '/api/health') return json(res, 200, { ok: true, version: 1 });

  // The interface itself carries no data and must load before the page knows its token; only the
  // API is gated.
  if (!path.startsWith('/api/')) {
    const file = path === '/' ? 'index.html' : path.slice(1);
    const full = join(PUBLIC, normalize(file));
    if (!full.startsWith(PUBLIC) || !existsSync(full)) return fail(res, 404, 'not found');
    const body = await readFile(full);
    res.writeHead(200, {
      'content-type': MIME[extname(full)] ?? 'application/octet-stream',
      'content-length': body.length,
      'cache-control': 'no-cache',
    });
    return res.end(body);
  }

  if (!authorised(req, url)) return fail(res, 401, 'token required');

  if (path === '/api/tasks' && req.method === 'GET') {
    return json(res, 200, await loadTasks());
  }
  if (path === '/api/tasks' && req.method === 'POST') {
    const data = await loadTasks();
    const task = cleanTask(JSON.parse(await readBody(req)));
    data.tasks.unshift(task);
    await saveTasks(data);
    return json(res, 201, task);
  }
  const taskMatch = path.match(/^\/api\/tasks\/([\w-]+)$/);
  if (taskMatch) {
    const data = await loadTasks();
    const index = data.tasks.findIndex((task) => task.id === taskMatch[1]);
    if (index === -1) return fail(res, 404, 'no such task');
    if (req.method === 'PATCH') {
      // Any field may change; the board does not police the meaning of a column. A card in
      // "Готово" should carry passing checks, but that is a convention the board shows, not a gate.
      const updated = cleanTask(JSON.parse(await readBody(req)), data.tasks[index]);
      data.tasks[index] = updated;
      await saveTasks(data);
      return json(res, 200, updated);
    }
    if (req.method === 'DELETE') {
      const [removed] = data.tasks.splice(index, 1);
      await saveTasks(data);
      return json(res, 200, removed);
    }
  }

  if (path === '/api/tree') {
    return json(res, 200, await tree(url.searchParams.get('path') ?? '.'));
  }
  if (path === '/api/file') {
    const rel = url.searchParams.get('path') ?? '';
    const full = safePath(rel);
    if (!full) return fail(res, 403, 'outside the viewable area');
    if (!existsSync(full)) return fail(res, 404, 'no such file');
    const text = await readFile(full, 'utf8');
    return json(res, 200, { path: rel, text });
  }
  if (path === '/api/changes') {
    return json(res, 200, await changes());
  }
  if (path === '/api/diff') {
    const rel = url.searchParams.get('path') ?? '';
    if (!safePath(rel)) return fail(res, 403, 'outside the viewable area');
    const tracked = await git(['diff', 'HEAD', '--', rel]);
    const text = tracked.trim() ? tracked : await git(['diff', '--no-index', '/dev/null', rel]);
    return json(res, 200, { path: rel, diff: text });
  }

  return fail(res, 404, 'not found');
}

const server = createServer((req, res) => {
  const url = new URL(req.url ?? '/', `http://${req.headers.host ?? 'localhost'}`);
  route(req, res, url).catch((error) => {
    console.error(`${req.method} ${url.pathname}: ${error.message}`);
    if (!res.headersSent) fail(res, 500, error.message);
  });
});

server.listen(PORT, HOST, () => {
  const shown = HOST === '0.0.0.0' ? 'localhost' : HOST;
  console.log(`workboard: http://${shown}:${PORT}${TOKEN ? `?token=${TOKEN}` : ''}`);
  console.log(`repository: ${ROOT}`);
});
