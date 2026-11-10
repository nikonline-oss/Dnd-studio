import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reviewSummary } from './public/review.js';

test('a failed check prevents a ready verdict even if other checks pass', () => {
  assert.deepEqual(reviewSummary({ checks: [{ state: 'pass' }, { state: 'fail' }] }), {
    state: 'fail', label: 'Есть ошибки', pass: 1, fail: 1, pending: 0, total: 2,
  });
});
test('all checks must pass before a task can be accepted', () => {
  assert.equal(reviewSummary({ checks: [{ state: 'pass' }] }).label, 'Можно принять');
  assert.equal(reviewSummary({ checks: [{ state: 'pass' }, { state: 'pending' }] }).state, 'pending');
});
test('an empty checklist is unverified rather than successful', () => {
  assert.equal(reviewSummary({ checks: [] }).state, 'pending');
});
