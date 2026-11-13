import { test } from 'node:test';
import assert from 'node:assert/strict';
import { buildSave, formFromView, parseView, summary, type Form, type Row } from '../src/settings-model.ts';

const row = (over: Partial<Row> = {}): Row => ({ name: 'REGION', secret: false, value: 'eu-west-1', hasValue: true, touched: false, ...over });
const form = (over: Partial<Form> = {}): Form => ({ fullAccess: true, githubUsername: '', tokenInput: '', removeToken: false, hasToken: false, rows: [], ...over });
const request = (f: Form) => { const built = buildSave(f); assert.ok('request' in built, 'problem' in built ? built.problem : ''); return built.request; };
const problem = (f: Form) => { const built = buildSave(f); assert.ok('problem' in built); return built.problem; };

test('a view from the backend is read strictly and never carries a secret value', () => {
  const view = parseView({ fullAccess: true, githubUsername: 'octocat', hasGithubToken: true,
    variables: [{ name: 'API_KEY', secret: true, value: null, hasValue: true }, { name: 'REGION', secret: false, value: 'eu', hasValue: true }] });
  assert.ok(view);
  assert.equal(view.variables[0].value, null);
  const rebuilt = formFromView(view);
  assert.equal(rebuilt.rows[0].value, '');
  assert.equal(rebuilt.rows[0].hasValue, true);
  assert.equal(rebuilt.tokenInput, '');
  for (const bad of [null, 3, {}, { fullAccess: 'yes' }, { fullAccess: true, githubUsername: '', hasGithubToken: false, variables: [{ name: 1 }] }]) assert.equal(parseView(bad), null);
});

test('an untouched row with a saved value is kept, an edited one is replaced', () => {
  const kept = request(form({ rows: [row()] }));
  assert.deepEqual(kept.variables[0].value, { op: 'keep' });
  const replaced = request(form({ rows: [row({ touched: true, value: 'us-east-1' })] }));
  assert.deepEqual(replaced.variables[0].value, { op: 'set', value: 'us-east-1' });
});

test('a new row needs a value, and an empty leftover row is dropped', () => {
  assert.match(problem(form({ rows: [row({ hasValue: false, value: '' })] })), /REGION needs a value/);
  assert.deepEqual(request(form({ rows: [row({ name: '', value: '', hasValue: false })] })).variables, []);
  const added = request(form({ rows: [row({ hasValue: false, value: 'x', touched: true })] }));
  assert.deepEqual(added.variables[0].value, { op: 'set', value: 'x' });
});

test('names are checked before anything is sent, ignoring case for duplicates', () => {
  assert.match(problem(form({ rows: [row({ name: '1ABC' })] })), /not a valid name/);
  assert.match(problem(form({ rows: [row({ name: 'A-B' })] })), /not a valid name/);
  assert.match(problem(form({ rows: [row({ name: '', value: 'x', hasValue: false })] })), /has no name/);
  assert.match(problem(form({ rows: [row({ name: 'Token' }), row({ name: 'TOKEN' })] })), /appears twice/);
});

test('the GitHub token is set, cleared or kept, and the username is checked', () => {
  assert.deepEqual(request(form({ tokenInput: '  ghp_abc  ' })).githubToken, { op: 'set', value: 'ghp_abc' });
  assert.deepEqual(request(form({ hasToken: true, removeToken: true })).githubToken, { op: 'clear' });
  assert.deepEqual(request(form({ hasToken: true })).githubToken, { op: 'keep' });
  assert.deepEqual(request(form({ hasToken: true, removeToken: true, tokenInput: 'ghp_new' })).githubToken, { op: 'set', value: 'ghp_new' });
  assert.match(problem(form({ tokenInput: 'has space' })), /spaces/);
  assert.match(problem(form({ githubUsername: '-bad' })), /username/);
  assert.match(problem(form({ githubUsername: 'a b' })), /username/);
  assert.equal(request(form({ githubUsername: ' octocat ' })).githubUsername, 'octocat');
});

test('the summary says what the next sessions get', () => {
  assert.equal(summary(form({ fullAccess: false })), 'agents keep their own protections');
  assert.equal(summary(form({ hasToken: true, rows: [row()] })), 'full access · GitHub sign-in · 1 variable');
  assert.equal(summary(form({ hasToken: true, removeToken: true })), 'full access');
  assert.equal(summary(form({ rows: [row(), row({ name: 'B' })] })), 'full access · 2 variables');
});
