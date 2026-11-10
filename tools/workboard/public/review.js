/** Acceptance is deliberately independent of the kanban column: only the owner closes a task. */
export function reviewSummary(task) {
  const pass = task.checks.filter((check) => check.state === 'pass').length;
  const fail = task.checks.filter((check) => check.state === 'fail').length;
  const pending = task.checks.length - pass - fail;
  const state = fail > 0 ? 'fail' : pass > 0 && pending === 0 ? 'pass' : 'pending';
  const label = { fail: 'Есть ошибки', pass: 'Можно принять', pending: 'Нужна проверка' }[state];
  return { state, label, pass, fail, pending, total: task.checks.length };
}
