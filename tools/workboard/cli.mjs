#!/usr/bin/env node
// Small CLI for the workboard, so progress can be recorded from a shell or a test run.
//
//   node tools/workboard/cli.mjs list [column]
//   node tools/workboard/cli.mjs move <task-id-or-prefix> <column>
//   node tools/workboard/cli.mjs check <task-id-or-prefix> <pass|fail|pending> <label> [note]
//   node tools/workboard/cli.mjs add <column> <title> [description]
//
// It talks to the running server when one is reachable and falls back to editing tasks.json, so it
// works both while the board is open in a browser and in a headless run.

import { readFile, writeFile, rename } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('.', import.meta.url)), '../..');
const TASKS = join(ROOT, 'docs/workboard/tasks.json');
const BASE = process.env.WORKBOARD_URL ?? 'http://127.0.0.1:8090';
const TOKEN = process.env.WORKBOARD_TOKEN ?? '';

const COLUMNS = ['backlog', 'doing', 'verify', 'done'];

async function load() {
  return JSON.parse(await readFile(TASKS, 'utf8'));
}

async function save(data) {
  const text = JSON.stringify({ version: 1, tasks: data.tasks }, null, 2) + '\n';
  await writeFile(`${TASKS}.tmp`, text);
  await rename(`${TASKS}.tmp`, TASKS);
}

function findTask(data, needle) {
  const exact = data.tasks.find((task) => task.id === needle);
  if (exact) return exact;
  const matches = data.tasks.filter((task) => task.id.startsWith(needle));
  if (matches.length === 1) return matches[0];
  if (matches.length === 0) throw new Error(`no task matches ${needle}`);
  throw new Error(`${needle} matches ${matches.length} tasks; use a longer prefix`);
}

/** Prefer the running server so an open board refreshes on its next fetch. */
async function patchRemote(task) {
  if (!TOKEN) return false;
  try {
    const response = await fetch(`${BASE}/api/tasks/${task.id}`, {
      method: 'PATCH',
      headers: { 'content-type': 'application/json', authorization: `Bearer ${TOKEN}` },
      body: JSON.stringify(task),
    });
    return response.ok;
  } catch {
    return false;
  }
}

async function update(needle, mutate) {
  const data = await load();
  const task = findTask(data, needle);
  mutate(task);
  task.updatedAt = new Date().toISOString();
  if (!(await patchRemote(task))) await save(data);
  console.log(`${task.id} ${task.column}: ${task.title}`);
  return task;
}

const [command, ...args] = process.argv.slice(2);

try {
  switch (command) {
    case 'list': {
      const data = await load();
      const wanted = args[0];
      for (const column of COLUMNS) {
        if (wanted && wanted !== column) continue;
        const items = data.tasks.filter((task) => task.column === column);
        console.log(`\n${column} (${items.length})`);
        for (const task of items) {
          const checks = task.checks.length
            ? ` [${task.checks.filter((c) => c.state === 'pass').length}/${task.checks.length}]`
            : '';
          console.log(`  ${task.id}  ${task.priority.padEnd(6)} ${task.title}${checks}`);
        }
      }
      break;
    }
    case 'move': {
      const [needle, column] = args;
      if (!COLUMNS.includes(column)) throw new Error(`column must be one of ${COLUMNS.join(', ')}`);
      await update(needle, (task) => {
        task.column = column;
      });
      break;
    }
    case 'check': {
      const [needle, state, label, note = ''] = args;
      if (!['pass', 'fail', 'pending'].includes(state)) {
        throw new Error('state must be pass, fail or pending');
      }
      if (!label) throw new Error('check needs a label, e.g. "cargo test -p dnd-game-core"');
      await update(needle, (task) => {
        const existing = task.checks.find((check) => check.label === label);
        if (existing) {
          existing.state = state;
          existing.note = note;
        } else {
          task.checks.push({ label, state, note });
        }
      });
      break;
    }
    case 'append': {
      // Appends a note to the description: what was done, what changed, what to look at.
      const [needle, ...words] = args;
      const text = words.join(' ');
      if (!text) throw new Error('append needs text');
      await update(needle, (task) => {
        task.description = task.description ? `${task.description}\n\n${text}` : text;
      });
      break;
    }
    case 'add': {
      const [column = 'backlog', title, description = ''] = args;
      if (!title) throw new Error('add needs a title');
      const data = await load();
      const task = {
        id: `t-${Math.random().toString(16).slice(2, 10)}`,
        title,
        description,
        column: COLUMNS.includes(column) ? column : 'backlog',
        priority: 'normal',
        kind: 'task',
        files: [],
        checks: [],
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      };
      data.tasks.unshift(task);
      await save(data);
      console.log(`${task.id} ${task.column}: ${task.title}`);
      break;
    }
    default:
      console.log(
        'usage: cli.mjs list [column] | move <id> <column> | check <id> <pass|fail|pending> <label> [note] | append <id> <text> | add <column> <title> [description]',
      );
      process.exitCode = command ? 1 : 0;
  }
} catch (error) {
  console.error(`error: ${error.message}`);
  process.exitCode = 1;
}
