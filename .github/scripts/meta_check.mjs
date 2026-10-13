// PR のメタ検査 (ADR 020 の 8 章 1)。差分の行数・変更ファイル数・ID の参照を調べる。
// 判定は純関数 evaluate に集め、入出力は main だけが行う。
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

export const LIMITS = { lines: 400, portedLines: 800, files: 15 };

const TEST_PATH = /(^|\/)tests\/|(^|\/)tests\.rs$|_tests\.rs$/;
const SOURCE_PATH =
  /\.(rs|wgsl|toml)$|(^|\/)panel\.(html|css)$|^\.github\/.*\.ya?ml$|^\.github\/scripts\/|^scripts\//;
const PORTED_FROM = /^Ported-from: (legacy-final|legacy-adr019-wip):\S+/m;
const REFS_LINE = /^Refs: (.*)$/gm;
const REF_ID = /A-[A-Za-z0-9-]+|#[0-9]+/g;

export const isDocPath = (path) => path.startsWith('docs/') || path.endsWith('.md');
export const isGithubPath = (path) => path.startsWith('.github/');
export const isCountedSource = (path) =>
  !isDocPath(path) && !TEST_PATH.test(path) && SOURCE_PATH.test(path);

// `git diff --numstat --no-renames` の出力を読む。バイナリ ("-") は 0 行として扱う。
export const parseNumstat = (text) =>
  text
    .split('\n')
    .filter((line) => line.trim() !== '')
    .map((line) => line.split('\t'))
    .map(([added, , ...path]) => ({ path: path.join('\t'), added: Number(added) || 0 }));

// 行頭が "Refs: " で、すぐ後に ID が続く行だけを対象にする。
export const refIds = (body) =>
  [...body.matchAll(REFS_LINE)]
    .map(([, rest]) => rest)
    .filter((rest) => new RegExp(`^(${REF_ID.source})`).test(rest))
    .flatMap((rest) => rest.match(REF_ID) ?? []);

// 受け入れ ID は、ACCEPTANCE.md の表の先頭の列にあるものだけを数える。
// 接頭辞だけ (A-R)、例示 (A-xx)、本文の中の言及は ID として扱わない。
const ID_CELL = /^\| `?(A-[A-Z]+[0-9-][A-Za-z0-9-]*)`?(?: \(\d+\))? \|/gm;
export const acceptanceIds = (acceptance) =>
  new Set([...acceptance.matchAll(ID_CELL)].map(([, id]) => id));

const checkLines = ({ files, body, labels }) => {
  const limit = PORTED_FROM.test(body) ? LIMITS.portedLines : LIMITS.lines;
  const lines = files
    .filter((f) => isCountedSource(f.path))
    .reduce((sum, f) => sum + f.added, 0);
  const exempt = labels.includes('size-exempt');
  return {
    name: 'lines',
    ok: exempt || lines <= limit,
    detail: `手書きのソースの追加行 ${lines} / 上限 ${limit}${exempt ? ' (size-exempt で免除)' : ''}`,
  };
};

const checkFiles = ({ files, labels }) => {
  const docsOnly = files.every((f) => isDocPath(f.path));
  const exempt = labels.includes('size-exempt') || labels.includes('files-exempt');
  const reason = docsOnly ? ' (文書だけの PR なので免除)' : exempt ? ' (ラベルで免除)' : '';
  return {
    name: 'files',
    ok: docsOnly || exempt || files.length <= LIMITS.files,
    detail: `変更ファイル ${files.length} / 上限 ${LIMITS.files}${reason}`,
  };
};

const checkRefs = ({ files, body, acceptance }) => {
  const exemptReason = files.every((f) => isDocPath(f.path))
    ? '文書だけの PR'
    : files.every((f) => isGithubPath(f.path))
      ? '.github だけの PR'
      : acceptance === null
        ? 'docs/ACCEPTANCE.md がまだ無い'
        : null;
  if (exemptReason !== null) {
    return { name: 'refs', ok: true, detail: `ID の参照は免除 (${exemptReason})` };
  }
  const ids = refIds(body);
  const known = acceptanceIds(acceptance);
  const unknown = ids.filter((id) => id.startsWith('A-') && !known.has(id));
  const detail =
    ids.length === 0
      ? '本文に "Refs: A-xx" か "Refs: #番号" の行が無い'
      : unknown.length > 0
        ? `docs/ACCEPTANCE.md に無い ID: ${unknown.join(', ')}`
        : `参照: ${ids.join(', ')}`;
  return { name: 'refs', ok: ids.length > 0 && unknown.length === 0, detail };
};

// input: { files: [{path, added}], body: string, labels: string[], acceptance: string | null }
export const evaluate = (input) => {
  const checks = [checkLines(input), checkFiles(input), checkRefs(input)];
  return { ok: checks.every((c) => c.ok), checks };
};

const main = () => {
  const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8'));
  const pr = event.pull_request;
  const numstat = execFileSync(
    'git',
    ['diff', '--numstat', '--no-renames', `${pr.base.sha}...${pr.head.sha}`],
    { encoding: 'utf8' },
  );
  const acceptancePath = 'docs/ACCEPTANCE.md';
  const result = evaluate({
    files: parseNumstat(numstat),
    body: (pr.body ?? '').replace(/\r\n/g, '\n'),
    labels: pr.labels.map((label) => label.name),
    acceptance: existsSync(acceptancePath) ? readFileSync(acceptancePath, 'utf8') : null,
  });
  result.checks.forEach((c) => console.log(`${c.ok ? 'OK  ' : 'FAIL'} ${c.name}: ${c.detail}`));
  process.exitCode = result.ok ? 0 : 1;
};

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  main();
}
