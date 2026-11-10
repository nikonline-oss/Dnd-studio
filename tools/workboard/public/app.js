// Workboard UI: a kanban board over docs/workboard/tasks.json plus a read-only view of the
// repository's docs and the working tree. No framework: the tool must stay dependency-free.
import { reviewSummary } from './review.js';

const params = new URLSearchParams(location.search);
const TOKEN = params.get('token') ?? '';
const auth = () => (TOKEN ? { authorization: `Bearer ${TOKEN}` } : {});

const KIND_LABEL = { task: 'задача', bug: 'баг', question: 'вопрос' };
const PRIO_LABEL = { high: 'высокий', normal: 'обычный', low: 'низкий' };
const CHECK_LABEL = { pass: 'пройдено', fail: 'провалено', pending: 'не проверено' };
const COLUMN_ORDER = ['backlog', 'doing', 'verify', 'done'];

const state = {
  columns: [],
  tasks: [],
  filter: '',
  view: 'board',
  openId: null,
  file: null,
};

const el = (id) => document.getElementById(id);

/** Russian plural form: 1 файл, 2 файла, 5 файлов. */
function plural(count, one, few, many) {
  if (count === 0) return '';
  const mod10 = count % 10;
  const mod100 = count % 100;
  const word =
    mod10 === 1 && mod100 !== 11
      ? one
      : mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)
        ? few
        : many;
  return `${count} ${word}`;
}

async function api(path, options = {}) {
  const response = await fetch(path, {
    ...options,
    headers: { 'content-type': 'application/json', ...auth(), ...(options.headers ?? {}) },
  });
  const body = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(body.error ?? `HTTP ${response.status}`);
  return body;
}

// ---------- markdown ----------

const escapeHtml = (text) =>
  text.replace(
    /[&<>"']/g,
    (ch) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[ch],
  );

function inline(text) {
  return escapeHtml(text)
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/(^|[^*])\*([^*]+)\*/g, '$1<em>$2</em>')
    .replace(/\[([^\]]+)\]\(([^)]+)\)/g, (_m, label, href) => {
      const safe = /^(https?:|#|\.{0,2}\/)/.test(href) ? href : '#';
      return `<a href="${safe}" target="_blank" rel="noreferrer">${label}</a>`;
    });
}

/**
 * Nested lists: an item owns the indented lines below it, and a deeper marker starts a sub-list.
 * Continuation lines are joined into the item's text, which is what the docs actually use.
 */
function parseList(lines, start) {
  const baseIndent = lines[start].search(/\S/);
  const ordered = /^\s*\d+\.\s+/.test(lines[start]);
  const items = [];
  let index = start;
  while (index < lines.length) {
    const match = lines[index].match(/^(\s*)([-*+]|\d+\.)\s+(.*)$/);
    if (!match) {
      // A blank line ends the list; an indented non-marker line continues the last item.
      if (lines[index].trim() === '') break;
      if (/^\s+\S/.test(lines[index]) && items.length) {
        items[items.length - 1].text += ` ${lines[index].trim()}`;
        index += 1;
        continue;
      }
      break;
    }
    const indent = match[1].length;
    if (indent < baseIndent) break;
    if (indent > baseIndent) {
      const nested = parseList(lines, index);
      if (items.length) items[items.length - 1].children.push(nested.html);
      index = nested.next;
      continue;
    }
    items.push({ text: match[3], children: [] });
    index += 1;
  }
  const tag = ordered ? 'ol' : 'ul';
  const html = `<${tag}>${items
    .map((item) => `<li>${inline(item.text)}${item.children.join('')}</li>`)
    .join('')}</${tag}>`;
  return { html, next: index };
}

/** Minimal markdown: headings, lists, code fences, tables, quotes, rules, paragraphs. */
function markdown(source) {
  const lines = source.replaceAll('\r\n', '\n').split('\n');
  const out = [];
  let index = 0;
  let paragraph = [];

  const flushParagraph = () => {
    if (paragraph.length) {
      out.push(`<p>${inline(paragraph.join(' '))}</p>`);
      paragraph = [];
    }
  };

  while (index < lines.length) {
    const line = lines[index];

    if (line.startsWith('```')) {
      flushParagraph();
      const language = line.slice(3).trim();
      const buffer = [];
      index += 1;
      while (index < lines.length && !lines[index].startsWith('```')) {
        buffer.push(lines[index]);
        index += 1;
      }
      index += 1;
      out.push(
        `<pre><code${language ? ` class="lang-${escapeHtml(language)}"` : ''}>${escapeHtml(buffer.join('\n'))}</code></pre>`,
      );
      continue;
    }

    const heading = line.match(/^(#{1,6})\s+(.*)$/);
    if (heading) {
      flushParagraph();
      const level = heading[1].length;
      out.push(`<h${level}>${inline(heading[2])}</h${level}>`);
      index += 1;
      continue;
    }

    if (/^\s*([-*_])\1\1+\s*$/.test(line)) {
      flushParagraph();
      out.push('<hr />');
      index += 1;
      continue;
    }

    // Table: header row, separator row, then body rows.
    if (line.includes('|') && /^\s*\|?[\s:-]+\|[\s:|-]*$/.test(lines[index + 1] ?? '')) {
      flushParagraph();
      const cells = (row) =>
        row
          .replace(/^\s*\|/, '')
          .replace(/\|\s*$/, '')
          .split('|')
          .map((cell) => cell.trim());
      const header = cells(line);
      index += 2;
      const rows = [];
      while (index < lines.length && lines[index].includes('|') && lines[index].trim() !== '') {
        rows.push(cells(lines[index]));
        index += 1;
      }
      out.push(
        `<table><thead><tr>${header.map((cell) => `<th>${inline(cell)}</th>`).join('')}</tr></thead><tbody>${rows
          .map((row) => `<tr>${row.map((cell) => `<td>${inline(cell)}</td>`).join('')}</tr>`)
          .join('')}</tbody></table>`,
      );
      continue;
    }

    if (/^\s*([-*+]|\d+\.)\s+/.test(line)) {
      flushParagraph();
      const parsed = parseList(lines, index);
      out.push(parsed.html);
      index = parsed.next;
      continue;
    }

    if (/^>\s?/.test(line)) {
      flushParagraph();
      const quoted = [];
      while (index < lines.length && /^>\s?/.test(lines[index])) {
        quoted.push(lines[index].replace(/^>\s?/, ''));
        index += 1;
      }
      out.push(`<blockquote>${inline(quoted.join(' '))}</blockquote>`);
      continue;
    }

    if (line.trim() === '') {
      flushParagraph();
      index += 1;
      continue;
    }

    paragraph.push(line.trim());
    index += 1;
  }
  flushParagraph();
  return out.join('\n');
}

// ---------- board ----------

function checksSummary(task) {
  const pass = task.checks.filter((check) => check.state === 'pass').length;
  const fail = task.checks.filter((check) => check.state === 'fail').length;
  const total = task.checks.length;
  if (total === 0) return { text: '', state: '' };
  const state = fail > 0 ? 'fail' : pass === total ? 'pass' : 'mixed';
  const mark = fail > 0 ? '✗' : pass === total ? '✓' : '•';
  return { text: `${mark} ${pass}/${total}`, state };
}

function matches(task) {
  if (!state.filter) return true;
  const needle = state.filter.toLowerCase();
  return (
    task.title.toLowerCase().includes(needle) ||
    task.description.toLowerCase().includes(needle) ||
    task.files.some((file) => file.toLowerCase().includes(needle))
  );
}

function renderBoard() {
  const board = el('board');
  board.innerHTML = '';
  const reviewed = state.tasks.filter((task) => task.column === 'verify').map(reviewSummary);
  el('review-overview').innerHTML = `<strong>Приёмка</strong>${[
    ['pass', 'Можно принять'],
    ['fail', 'Есть ошибки'],
    ['pending', 'Нужна проверка'],
  ]
    .map(
      ([kind, label]) =>
        `<span class="review-verdict" data-state="${kind}">${label}: ${reviewed.filter((result) => result.state === kind).length}</span>`,
    )
    .join('')}<span class="muted">Обновляется автоматически</span>`;
  for (const id of COLUMN_ORDER) {
    const column = state.columns.find((c) => c.id === id);
    if (!column) continue;
    const tasks = state.tasks.filter((task) => task.column === id && matches(task));
    const node = document.createElement('section');
    node.className = 'column';
    node.dataset.column = id;
    node.innerHTML = `
      <header class="column-head">
        <h2>${escapeHtml(column.title)}</h2>
        <span class="count">${tasks.length}</span>
      </header>
      <div class="column-body"></div>`;
    const body = node.querySelector('.column-body');

    for (const task of tasks) {
      const card = el('card-template').content.firstElementChild.cloneNode(true);
      card.dataset.id = task.id;
      card.dataset.priority = task.priority;
      card.dataset.column = task.column;
      card.querySelector('.kind').textContent = KIND_LABEL[task.kind] ?? task.kind;
      card.querySelector('.kind').dataset.kind = task.kind;
      card.querySelector('.prio').textContent = PRIO_LABEL[task.priority] ?? '';
      card.querySelector('.card-title').textContent = task.title;
      card.querySelector('.card-desc').textContent = task.description;
      const summary = checksSummary(task);
      const checks = card.querySelector('.checks');
      checks.textContent = summary.text;
      checks.dataset.state = summary.state;
      if (task.column === 'verify') {
        const verdict = reviewSummary(task);
        const badge = document.createElement('span');
        badge.className = 'review-verdict';
        badge.dataset.state = verdict.state;
        badge.textContent = verdict.label;
        card.querySelector('.card-head').prepend(badge);
        card.dataset.review = verdict.state;
      }
      card.querySelector('.files').textContent = plural(
        task.files.length,
        'файл',
        'файла',
        'файлов',
      );
      card.addEventListener('click', () => openTask(task.id));
      card.addEventListener('dragstart', (event) => {
        event.dataTransfer.setData('text/plain', task.id);
        event.dataTransfer.effectAllowed = 'move';
      });
      body.append(card);
    }

    if (tasks.length === 0) {
      const empty = document.createElement('p');
      empty.className = 'empty';
      empty.textContent = column.hint;
      body.append(empty);
    }

    node.addEventListener('dragover', (event) => {
      event.preventDefault();
      node.classList.add('over');
    });
    node.addEventListener('dragleave', () => node.classList.remove('over'));
    node.addEventListener('drop', async (event) => {
      event.preventDefault();
      node.classList.remove('over');
      const id = event.dataTransfer.getData('text/plain');
      const task = state.tasks.find((entry) => entry.id === id);
      if (!task || task.column === id) {
        if (task && task.column === node.dataset.column) return;
      }
      if (!task) return;
      await moveTask(task, node.dataset.column);
    });
    board.append(node);
  }
}

async function moveTask(task, column) {
  try {
    const updated = await api(`/api/tasks/${task.id}`, {
      method: 'PATCH',
      body: JSON.stringify({ ...task, column }),
    });
    Object.assign(task, updated);
    renderBoard();
  } catch (error) {
    alert(`Не удалось переместить: ${error.message}`);
  }
}

// ---------- drawer ----------

function openTask(id) {
  state.openId = id;
  const task = state.tasks.find((entry) => entry.id === id);
  if (!task) return;
  const drawer = el('drawer');
  drawer.classList.remove('hidden');
  drawer.innerHTML = `
    <div class="drawer-head">
      <h2>Задача</h2>
      <button class="ghost" id="close-drawer" aria-label="Закрыть">✕</button>
    </div>
    <label class="field"><span>Название</span><input id="t-title" maxlength="200" /></label>
    <div class="row">
      <label class="field" style="flex:1"><span>Колонка</span>
        <select id="t-column">
          ${state.columns.map((c) => `<option value="${c.id}">${escapeHtml(c.title)}</option>`).join('')}
        </select>
      </label>
      <label class="field" style="flex:1"><span>Тип</span>
        <select id="t-kind">
          ${Object.entries(KIND_LABEL)
            .map(([value, label]) => `<option value="${value}">${label}</option>`)
            .join('')}
        </select>
      </label>
      <label class="field" style="flex:1"><span>Приоритет</span>
        <select id="t-priority">
          ${Object.entries(PRIO_LABEL)
            .map(([value, label]) => `<option value="${value}">${label}</option>`)
            .join('')}
        </select>
      </label>
    </div>
    <label class="field"><span>Описание</span><textarea id="t-desc" rows="6"></textarea></label>
    <label class="field"><span>Файлы (по одному в строке, относительно корня репозитория)</span>
      <textarea id="t-files" rows="3" placeholder="docs/adr/0049-character-visibility.md"></textarea>
    </label>
    <div class="field"><span>Проверки</span><div class="checks-list" id="t-checks"></div>
      <button class="ghost" id="add-check">+ проверка</button>
    </div>
    <div class="field"><span>Файлы задачи</span><div class="file-links" id="t-file-links"></div></div>
    <div class="drawer-actions">
      <button class="danger" id="delete-task">Удалить</button>
      <button class="primary" id="save-task">Сохранить</button>
    </div>`;

  drawer.querySelector('#t-title').value = task.title;
  drawer.querySelector('#t-column').value = task.column;
  drawer.querySelector('#t-kind').value = task.kind;
  drawer.querySelector('#t-priority').value = task.priority;
  drawer.querySelector('#t-desc').value = task.description;
  drawer.querySelector('#t-files').value = task.files.join('\n');

  const checksNode = drawer.querySelector('#t-checks');
  const renderChecks = (checks) => {
    checksNode.innerHTML = '';
    checks.forEach((check, index) => {
      const row = document.createElement('div');
      row.className = `check ${check.state}`;
      row.innerHTML = `
        <select aria-label="Состояние проверки">
          ${Object.entries(CHECK_LABEL)
            .map(
              ([value, label]) =>
                `<option value="${value}" ${value === check.state ? 'selected' : ''}>${label}</option>`,
            )
            .join('')}
        </select>
        <div style="display:grid;gap:4px">
          <input class="check-label" placeholder="Что проверено" />
          <input class="check-note" placeholder="Результат: команда, вывод, наблюдение" />
        </div>`;
      row.querySelector('.check-label').value = check.label;
      row.querySelector('.check-note').value = check.note;
      row.querySelector('select').addEventListener('change', (event) => {
        row.className = `check ${event.target.value}`;
      });
      row.dataset.index = String(index);
      checksNode.append(row);
    });
  };
  const collectChecks = () =>
    [...checksNode.querySelectorAll('.check')]
      .map((row) => ({
        state: row.querySelector('select').value,
        label: row.querySelector('.check-label').value,
        note: row.querySelector('.check-note').value,
      }))
      .filter((check) => check.label.trim() !== '' || check.note.trim() !== '');
  renderChecks(task.checks);

  drawer.querySelector('#add-check').addEventListener('click', () => {
    const checks = collectChecks();
    checks.push({ state: 'pending', label: '', note: '' });
    renderChecks(checks);
  });

  const renderLinks = (files) => {
    const holder = drawer.querySelector('#t-file-links');
    holder.innerHTML = '';
    if (files.length === 0) {
      holder.innerHTML = '<span class="muted">Файлы не указаны.</span>';
      return;
    }
    for (const file of files) {
      const button = document.createElement('button');
      button.className = 'ghost';
      button.textContent = file;
      button.addEventListener('click', () => {
        closeDrawer();
        switchView('files');
        void openFile(file);
      });
      holder.append(button);
    }
  };
  const refreshLinks = () => {
    const files = drawer
      .querySelector('#t-files')
      .value.split('\n')
      .map((line) => line.trim())
      .filter(Boolean);
    renderLinks(files);
    return files;
  };
  drawer.querySelector('#t-files').addEventListener('input', refreshLinks);
  refreshLinks();

  drawer.querySelector('#close-drawer').addEventListener('click', closeDrawer);
  drawer.querySelector('#save-task').addEventListener('click', async () => {
    const files = refreshLinks();
    try {
      const updated = await api(`/api/tasks/${task.id}`, {
        method: 'PATCH',
        body: JSON.stringify({
          title: drawer.querySelector('#t-title').value,
          column: drawer.querySelector('#t-column').value,
          kind: drawer.querySelector('#t-kind').value,
          priority: drawer.querySelector('#t-priority').value,
          description: drawer.querySelector('#t-desc').value,
          files,
          checks: collectChecks(),
        }),
      });
      Object.assign(task, updated);
      renderBoard();
      closeDrawer();
    } catch (error) {
      alert(`Не удалось сохранить: ${error.message}`);
    }
  });
  drawer.querySelector('#delete-task').addEventListener('click', async () => {
    if (!confirm(`Удалить «${task.title}»?`)) return;
    await api(`/api/tasks/${task.id}`, { method: 'DELETE' });
    state.tasks = state.tasks.filter((entry) => entry.id !== task.id);
    renderBoard();
    closeDrawer();
  });
}

function closeDrawer() {
  state.openId = null;
  el('drawer').classList.add('hidden');
}

function newTask() {
  const title = prompt('Название задачи');
  if (!title) return;
  api('/api/tasks', {
    method: 'POST',
    body: JSON.stringify({ title, column: 'backlog', kind: 'task' }),
  })
    .then((task) => {
      state.tasks.unshift(task);
      renderBoard();
      openTask(task.id);
    })
    .catch((error) => alert(`Не удалось создать: ${error.message}`));
}

// ---------- files ----------

async function loadTree() {
  const nodes = await api('/api/tree?path=docs');
  el('tree').innerHTML = '';
  el('tree').append(renderTree(nodes));
}

function renderTree(nodes) {
  const list = document.createElement('ul');
  for (const node of nodes) {
    const item = document.createElement('li');
    if (node.type === 'dir') {
      item.className = 'dir';
      const label = document.createElement('span');
      label.textContent = node.name;
      item.append(label, renderTree(node.children ?? []));
    } else {
      const button = document.createElement('button');
      button.textContent = node.name;
      button.dataset.path = node.path;
      button.addEventListener('click', () => {
        el('tree')
          .querySelectorAll('button.active')
          .forEach((b) => b.classList.remove('active'));
        button.classList.add('active');
        void openFile(node.path);
      });
      item.append(button);
    }
    list.append(item);
  }
  return list;
}

async function openFile(path) {
  try {
    const file = await api(`/api/file?path=${encodeURIComponent(path)}`);
    state.file = file;
    const doc = el('doc');
    doc.innerHTML = '';
    const head = document.createElement('div');
    head.className = 'muted';
    head.style.marginBottom = '12px';
    head.textContent = file.path;
    doc.append(head);
    const body = document.createElement('div');
    if (path.endsWith('.md')) {
      body.innerHTML = markdown(file.text);
    } else {
      const pre = document.createElement('pre');
      pre.textContent = file.text;
      body.append(pre);
    }
    doc.append(body);
  } catch (error) {
    el('doc').innerHTML = `<p class="muted">Не удалось открыть: ${escapeHtml(error.message)}</p>`;
  }
}

// ---------- changes ----------

async function loadChanges() {
  const changes = await api('/api/changes');
  const holder = el('change-list');
  holder.innerHTML = '';
  const summary = document.createElement('div');
  summary.className = 'muted';
  summary.style.cssText = 'padding:6px;font-size:12px;line-height:1.5';
  summary.textContent = `${changes.branch} · ${changes.head} · ${changes.files.length} изменённых файлов`;
  holder.append(summary);
  for (const file of changes.files) {
    const button = document.createElement('button');
    button.className = 'change';
    button.innerHTML = `<span class="code">${escapeHtml(file.code || '?')}</span><span>${escapeHtml(file.path)}</span>`;
    button.addEventListener('click', () => {
      holder.querySelectorAll('button.active').forEach((b) => b.classList.remove('active'));
      button.classList.add('active');
      void openDiff(file.path);
    });
    holder.append(button);
  }
  if (changes.files.length === 0) {
    const empty = document.createElement('p');
    empty.className = 'empty';
    empty.textContent = 'Рабочее дерево чистое.';
    holder.append(empty);
  }
}

async function openDiff(path) {
  try {
    const result = await api(`/api/diff?path=${encodeURIComponent(path)}`);
    const diff = el('diff');
    diff.textContent = result.diff.trim() || 'Изменений в файле нет (возможно, новый файл пуст).';
  } catch (error) {
    el('diff').textContent = `Не удалось получить diff: ${error.message}`;
  }
}

// ---------- shell ----------

function switchView(view) {
  state.view = view;
  document
    .querySelectorAll('.tab')
    .forEach((tab) => tab.classList.toggle('active', tab.dataset.view === view));
  el('board').classList.toggle('hidden', view !== 'board');
  el('review-overview').classList.toggle('hidden', view !== 'board');
  el('files').classList.toggle('hidden', view !== 'files');
  el('changes').classList.toggle('hidden', view !== 'changes');
  if (view === 'files' && !el('tree').childElementCount) void loadTree();
  if (view === 'changes') void loadChanges();
}

async function start() {
  document.querySelectorAll('.tab').forEach((tab) => {
    tab.addEventListener('click', () => switchView(tab.dataset.view));
  });
  el('new-task').addEventListener('click', newTask);
  el('filter').addEventListener('input', (event) => {
    state.filter = event.target.value;
    renderBoard();
  });
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') closeDrawer();
  });

  try {
    const data = await api('/api/tasks');
    state.columns = data.columns;
    state.tasks = data.tasks;
    renderBoard();
  } catch (error) {
    el('board').innerHTML =
      `<p class="empty">Не удалось загрузить задачи: ${escapeHtml(error.message)}${
        TOKEN ? '' : ' — возможно, нужен токен: откройте адрес с ?token=…'
      }</p>`;
  }
  try {
    const changes = await api('/api/changes');
    el('branch').textContent = `${changes.branch} · ${changes.files.length} изменений`;
  } catch {
    el('branch').textContent = '';
  }
  // Refresh the board after CLI updates without replacing an open drawer's unsaved edits.
  let refreshing = false;
  setInterval(async () => {
    if (document.hidden || refreshing || document.querySelector('.card:active')) return;
    refreshing = true;
    try {
      const data = await api('/api/tasks');
      if (JSON.stringify(data.tasks) !== JSON.stringify(state.tasks)) {
        state.tasks = data.tasks;
        state.columns = data.columns;
        renderBoard();
      }
    } catch {
      // Preserve the last successful view during a transient connection failure.
      el('branch').textContent = 'Не удалось обновить · повтор через 5 с';
    } finally {
      refreshing = false;
    }
  }, 5000);
}

void start();
