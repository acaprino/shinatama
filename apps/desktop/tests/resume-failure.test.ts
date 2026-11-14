import { test } from 'node:test';
import assert from 'node:assert/strict';
import { resumeFailureCode, resumeFailureText } from '../src/resume-failure.ts';

test('the code is read from the backend error object or a bare string', () => {
  assert.equal(resumeFailureCode({ code: 'resources' }), 'resources');
  assert.equal(resumeFailureCode({ issue: 'busy' }), 'busy');
  assert.equal(resumeFailureCode('owned_elsewhere'), 'owned_elsewhere');
  assert.equal(resumeFailureCode(new Error('x')), null);
  assert.equal(resumeFailureCode(null), null);
});

test('a memory refusal says so and that nothing was started', () => {
  const text = resumeFailureText({ code: 'resources' });
  assert.match(text, /memory/i);
  assert.match(text, /nothing was started/i);
});

test('every backend code of thread_resume has its own sentence', () => {
  const codes = ['resources', 'reader_blocked', 'busy', 'owned_elsewhere', 'source_changed', 'conflict', 'unbound', 'native_missing', 'native_unreachable',
    'project_unavailable', 'unsupported_harness', 'invalid_request'];
  const generic = resumeFailureText({ code: 'unavailable' });
  for (const code of codes) assert.notEqual(resumeFailureText({ code }), generic, code);
});

test('an unknown failure keeps the conversation and offers a retry', () => {
  assert.match(resumeFailureText({ code: 'unavailable' }), /saved conversation is kept/i);
  assert.match(resumeFailureText(undefined), /saved conversation is kept/i);
});
