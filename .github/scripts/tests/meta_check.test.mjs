// meta_check.mjs の単体テスト。`node --test .github/scripts/tests/` で実行する。
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  parseNumstat,
  isCountedSource,
  isDocPath,
  refIds,
  evaluate,
} from '../meta_check.mjs';

const file = (path, added = 1) => ({ path, added });
const failed = (result) => result.checks.filter((c) => !c.ok).map((c) => c.name);

test('parseNumstat はパスと追加行数を読み、バイナリは 0 行にする', () => {
  const text = '12\t3\tcrates/doc/src/lib.rs\n-\t-\tassets/golden.png\n0\t9\tREADME.md\n';
  assert.deepEqual(parseNumstat(text), [
    { path: 'crates/doc/src/lib.rs', added: 12 },
    { path: 'assets/golden.png', added: 0 },
    { path: 'README.md', added: 0 },
  ]);
});

test('行数を数えるのは手書きのソースだけ', () => {
  const counted = [
    'crates/doc/src/lib.rs',
    'crates/gpu/src/present.wgsl',
    'Cargo.toml',
    'panels/tool-palette/panel.html',
    'panels/tool-palette/panel.css',
    '.github/workflows/meta-check.yml',
    '.github/scripts/meta_check.mjs',
    'scripts/build-ui-wasm.sh',
  ];
  const notCounted = [
    'Cargo.lock',
    'docs/PRODUCT.md',
    'README.md',
    'crates/doc/src/tests.rs',
    'crates/doc/src/apply_tests.rs',
    'crates/doc/tests/undo.rs',
    '.github/scripts/tests/meta_check.test.mjs',
    'assets/golden.png',
    'panels/tool-palette/panel.meta.json',
  ];
  counted.forEach((p) => assert.equal(isCountedSource(p), true, p));
  notCounted.forEach((p) => assert.equal(isCountedSource(p), false, p));
});

test('文書のパスは docs/ の下と *.md', () => {
  assert.equal(isDocPath('docs/adr/020-plan.md'), true);
  assert.equal(isDocPath('docs/adr/figure.png'), true);
  assert.equal(isDocPath('CLAUDE.md'), true);
  assert.equal(isDocPath('crates/doc/src/lib.rs'), false);
});

test('refIds は行頭の Refs: から ID を取り出す', () => {
  const body = 'やったこと\nRefs: A-R1, A-PH2-03 #12\n本文中の Refs: A-R9 は数えない\nRefs: #7\n';
  assert.deepEqual(refIds(body), ['A-R1', 'A-PH2-03', '#12', '#7']);
  assert.deepEqual(refIds('Refs: なし'), []);
  assert.deepEqual(refIds(''), []);
});

test('上限以内で ID を参照していれば合格する', () => {
  const result = evaluate({
    files: [file('crates/doc/src/lib.rs', 400), file('crates/doc/src/tests.rs', 900)],
    body: 'Refs: A-R1',
    labels: [],
    acceptance: '| A-R1 | ... |',
  });
  assert.equal(result.ok, true);
});

test('手書きのソースが 400 行を超えると不合格になる', () => {
  const result = evaluate({
    files: [file('crates/doc/src/lib.rs', 401)],
    body: 'Refs: A-R1',
    labels: [],
    acceptance: '| A-R1 | 条件 |',
  });
  assert.deepEqual(failed(result), ['lines']);
});

test('Ported-from がある移植 PR は 800 行まで', () => {
  const input = {
    files: [file('crates/pen-io/src/abr.rs', 800)],
    body: 'Refs: #3\nPorted-from: legacy-final:crates/pen-io/src/abr.rs',
    labels: [],
    acceptance: '| A-R1 | 条件 |',
  };
  assert.equal(evaluate(input).ok, true);
  assert.deepEqual(
    failed(evaluate({ ...input, files: [file('crates/pen-io/src/abr.rs', 801)] })),
    ['lines'],
  );
  const wrongTag = { ...input, body: 'Refs: #3\nPorted-from: main:crates/pen-io/src/abr.rs' };
  assert.deepEqual(failed(evaluate(wrongTag)), ['lines']);
});

test('変更ファイルが 15 を超えると不合格になる', () => {
  const files = Array.from({ length: 16 }, (_, i) => file(`crates/doc/src/m${i}.rs`, 1));
  const result = evaluate({ files, body: 'Refs: A-R1', labels: [], acceptance: '| A-R1 | 条件 |' });
  assert.deepEqual(failed(result), ['files']);
});

test('文書だけの PR はファイル数と ID の参照を免除する', () => {
  const files = Array.from({ length: 30 }, (_, i) => file(`docs/adr/00${i}.md`, 50));
  const result = evaluate({ files, body: '', labels: [], acceptance: '| A-R1 | 条件 |' });
  assert.equal(result.ok, true);
});

test('.github だけの PR は ID の参照を免除するが、行数とファイル数は数える', () => {
  const ok = evaluate({
    files: [file('.github/workflows/meta-check.yml', 30)],
    body: '',
    labels: [],
    acceptance: '| A-R1 | 条件 |',
  });
  assert.equal(ok.ok, true);
  const tooLong = evaluate({
    files: [file('.github/scripts/meta_check.mjs', 401)],
    body: '',
    labels: [],
    acceptance: '| A-R1 | 条件 |',
  });
  assert.deepEqual(failed(tooLong), ['lines']);
});

test('ACCEPTANCE.md ができる前は ID の参照を免除する', () => {
  const result = evaluate({
    files: [file('crates/doc/src/lib.rs', 10)],
    body: '',
    labels: [],
    acceptance: null,
  });
  assert.equal(result.ok, true);
});

test('ID を参照しない PR は不合格になる', () => {
  const result = evaluate({
    files: [file('crates/doc/src/lib.rs', 10)],
    body: '説明だけ',
    labels: [],
    acceptance: '| A-R1 | 条件 |',
  });
  assert.deepEqual(failed(result), ['refs']);
});

test('ACCEPTANCE.md に無い受け入れ ID は不合格になる', () => {
  const input = {
    files: [file('crates/doc/src/lib.rs', 10)],
    labels: [],
    acceptance: '| A-R1 | ... |\n| A-R12a | ... |',
  };
  assert.deepEqual(failed(evaluate({ ...input, body: 'Refs: A-R99' })), ['refs']);
  // A-R1 は A-R12a の一部としてではなく、ID 全体で照合する
  assert.deepEqual(failed(evaluate({ ...input, body: 'Refs: A-R12' })), ['refs']);
  assert.equal(evaluate({ ...input, body: 'Refs: A-R12a' }).ok, true);
  assert.equal(evaluate({ ...input, body: 'Refs: #45' }).ok, true);
});

test('size-exempt は行数とファイル数を、files-exempt はファイル数だけを免除する', () => {
  const files = Array.from({ length: 20 }, (_, i) => file(`crates/doc/src/m${i}.rs`, 100));
  const base = { files, body: 'Refs: A-R1', acceptance: '| A-R1 | 条件 |' };
  assert.deepEqual(failed(evaluate({ ...base, labels: [] })), ['lines', 'files']);
  assert.deepEqual(failed(evaluate({ ...base, labels: ['files-exempt'] })), ['lines']);
  assert.equal(evaluate({ ...base, labels: ['size-exempt'] }).ok, true);
});

test('免除ラベルは ID の参照を免除しない', () => {
  const result = evaluate({
    files: [file('crates/doc/src/lib.rs', 10)],
    body: '',
    labels: ['size-exempt'],
    acceptance: '| A-R1 | 条件 |',
  });
  assert.deepEqual(failed(result), ['refs']);
});

test('PR テンプレートを書き換えないままでは、ID の参照にも移植の指定にもならない', async () => {
  const { readFileSync } = await import('node:fs');
  const template = readFileSync(new URL('../../pull_request_template.md', import.meta.url), 'utf8');
  assert.deepEqual(refIds(template), []);
  const result = evaluate({
    files: [file('crates/doc/src/lib.rs', 401)],
    body: template,
    labels: [],
    acceptance: '| A-R1 | 条件 |',
  });
  assert.deepEqual(failed(result), ['lines', 'refs']);
});

test('受け入れ ID として数えるのは、ACCEPTANCE.md の表の先頭の列にある ID だけ', async () => {
  const { acceptanceIds } = await import('../meta_check.mjs');
  const acceptance = [
    '| A-R | 接頭辞の説明 | 23 |',
    '| A-SPK | スパイク | 5 |',
    'PR の本文には `Refs: A-xx` のように書く。原因は A-R12a と同じ。',
    '| ID | 条件 | 状態 |',
    '|---|---|---|',
    '| A-R1 | 画素が残る | 未着手 |',
    '| A-R4-png | PNG に線が入る | 未着手 |',
    '| A-SPK-a (1) | IME の未確定文字列が表示される | 未 |',
    '| A-SPK-a (2) | 候補窓が近くに出る | 未 |',
    '| `A-PH2-03` | 障害を注入するとトーストが出る | 未着手 |',
  ].join('\n');
  assert.deepEqual([...acceptanceIds(acceptance)].sort(), ['A-PH2-03', 'A-R1', 'A-R4-png', 'A-SPK-a']);

  const input = { files: [file('crates/doc/src/lib.rs', 10)], labels: [], acceptance };
  // 接頭辞だけ、例示、分ける前の親の ID は、本文にあっても通さない
  ['A-R', 'A-xx', 'A-R12a', 'A-R4', 'A-SPK'].forEach((id) =>
    assert.deepEqual(failed(evaluate({ ...input, body: `Refs: ${id}` })), ['refs'], id),
  );
  ['A-R1', 'A-R4-png', 'A-SPK-a', 'A-PH2-03'].forEach((id) =>
    assert.equal(evaluate({ ...input, body: `Refs: ${id}` }).ok, true, id),
  );
});
