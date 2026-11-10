/** Acceptance is deliberately independent of the kanban column: only the owner closes a task. */
export function reviewSummary(task) {
  const pass = task.checks.filter((check) => check.state === 'pass').length;
  const fail = task.checks.filter((check) => check.state === 'fail').length;
  const pending = task.checks.length - pass - fail;
  return { state: 'pending', label: 'Нужна проверка', pass, fail, pending, total: task.checks.length };
}
