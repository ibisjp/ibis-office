// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// The loader of the blocks of fonts (subset-fetch.ts, GitHub issue #157 stage 2): the blocks a manifest lists are fetched by font and key, checked, kept; the
// same block asked for twice is one fetch; a damaged block is fetched again; one that cannot be had is not asked for again; what the manifest does not list is
// never fetched; the fonts and blocks are found by typeface and character.
import test from 'node:test';
import assert from 'node:assert/strict';
import { SubsetLoader, blockOf, fontFor, subsetsOf, type BlockStore, type SubsetEnv, type Subsets } from '../subset-fetch.js';

const BASE = new URL('https://example.com/app/');
const sha = async (b: Uint8Array): Promise<string> => [...new Uint8Array(await crypto.subtle.digest('SHA-256', b as BufferSource))].map((x) => x.toString(16).padStart(2, '0')).join('');
const same = (a: Uint8Array, b: Uint8Array): boolean => a.length === b.length && a.every((v, i) => v === b[i]);

async function build() {
  const bytes = { base: new Uint8Array([1, 2, 3, 4, 5]), '0001': new Uint8Array([9, 8, 7]) };
  const subsets: Subsets = {
    schema: 1,
    fonts: [
      {
        id: 'F',
        typeface: ['Font One', 'フォント'],
        license: 'SIL OFL 1.1',
        licenseFile: 'F/OFL.txt',
        blocks: [
          { key: 'base', family: 'ibis F base', file: 'F/base.aaa.ttf', size: 5, sha256: await sha(bytes.base), ranges: [[32, 126], [0x3040, 0x30ff]], boot: true },
          { key: '0001', family: 'ibis F 0001', file: 'F/0001.bbb.ttf', size: 3, sha256: await sha(bytes['0001']), ranges: [[0x4e00, 0x51ff]] },
        ],
      },
    ],
  };
  const files = new Map([
    ['https://example.com/app/F/base.aaa.ttf', bytes.base],
    ['https://example.com/app/F/0001.bbb.ttf', bytes['0001']],
  ]);
  return { subsets, files, bytes };
}

function env(files: Map<string, Uint8Array>, asked: string[], over: Partial<SubsetEnv> = {}, warnings: string[] = []): SubsetEnv {
  return {
    fetch: (async (input: RequestInfo | URL) => {
      asked.push(String(input));
      const found = files.get(String(input));
      return found ? new Response(found as BodyInit) : new Response('', { status: 404 });
    }) as typeof fetch,
    subtle: crypto.subtle,
    base: BASE,
    warn: (m) => void warnings.push(m),
    backoffMs: 1,
    ...over,
  };
}

function memoryStore(): BlockStore & { kept: Map<string, Uint8Array>; pruned: string[][] } {
  const kept = new Map<string, Uint8Array>();
  const pruned: string[][] = [];
  return { kept, pruned, get: async (k) => kept.get(k), put: async (k, v) => void kept.set(k, v), prune: async (keep) => void pruned.push([...keep]) };
}

test('a block is found by the typeface a document names and by the character', async () => {
  const { subsets } = await build();
  const font = fontFor(subsets, ' font one ')!;
  assert.equal(font.id, 'F');
  assert.equal(fontFor(subsets, 'フォント')?.id, 'F');
  assert.equal(fontFor(subsets, 'Another'), undefined);
  assert.equal(blockOf(font, 0x41)?.key, 'base');
  assert.equal(blockOf(font, 0x4e9c)?.key, '0001');
  assert.equal(blockOf(font, 0x5200), undefined, 'a character no block has');
  assert.equal(subsetsOf({ subsets }), subsets);
  assert.equal(subsetsOf({ subsets: { schema: 2, fonts: [] } }), null);
  assert.equal(subsetsOf(null), null);
});

test('the blocks of the start are the base blocks', async () => {
  const { subsets, files } = await build();
  assert.deepEqual(new SubsetLoader(subsets, env(files, [])).bootBlocks(), [{ fontId: 'F', key: 'base' }]);
});

test('a block is fetched, checked and kept; asked for again it is not fetched; at the same time it is one fetch', async () => {
  const { subsets, files, bytes } = await build();
  const asked: string[] = [];
  const store = memoryStore();
  const loader = new SubsetLoader(subsets, env(files, asked, { store }));
  const [a, b] = await Promise.all([loader.fetchBlock('F', '0001'), loader.fetchBlock('F', '0001')]);
  assert.ok(same(a, bytes['0001']) && same(b, bytes['0001']));
  assert.equal(asked.length, 1, 'one fetch for two requests');
  assert.ok(same(await loader.fetchBlock('F', '0001'), bytes['0001']));
  assert.equal(store.kept.size, 1, 'kept by its SHA-256');
  // Another page: the store has it, so nothing is fetched.
  const asked2: string[] = [];
  const again = new SubsetLoader(subsets, env(files, asked2, { store }));
  assert.ok(same(await again.fetchBlock('F', '0001'), bytes['0001']));
  assert.equal(asked2.length, 0);
  assert.deepEqual(again.taken(), ['F/0001']);
});

test('a damaged block in the store or on the way is not used; it is fetched again', async () => {
  const { subsets, files, bytes } = await build();
  const store = memoryStore();
  store.kept.set(subsets.fonts[0].blocks[1].sha256, new Uint8Array([0, 0, 0]));
  const asked: string[] = [];
  let first = true;
  const e = env(files, asked, { store });
  const inner = e.fetch;
  e.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    if (first) {
      first = false;
      return new Response(new Uint8Array([9, 8, 0]) as BodyInit);
    }
    return inner(input, init);
  }) as typeof fetch;
  const got = await new SubsetLoader(subsets, e).fetchBlock('F', '0001');
  assert.ok(same(got, bytes['0001']));
  assert.ok(same(store.kept.get(subsets.fonts[0].blocks[1].sha256)!, bytes['0001']), 'the good block replaced the damaged one');
});

test('a block that cannot be had is tried 3 more times, then not asked for again until forget', async () => {
  const { subsets } = await build();
  const asked: string[] = [];
  const warnings: string[] = [];
  const loader = new SubsetLoader(subsets, env(new Map(), asked, {}, warnings));
  await assert.rejects(loader.fetchBlock('F', 'base'), /HTTP 404/);
  assert.equal(asked.length, 4, 'one try and 3 more');
  await assert.rejects(loader.fetchBlock('F', 'base'), /HTTP 404/);
  assert.equal(asked.length, 4, 'not asked for again');
  assert.equal(warnings.length, 1);
  loader.forget();
  await assert.rejects(loader.fetchBlock('F', 'base'), /HTTP 404/);
  assert.equal(asked.length, 8);
});

test('what the manifest does not list is never fetched', async () => {
  const { subsets, files } = await build();
  const asked: string[] = [];
  const loader = new SubsetLoader(subsets, env(files, asked));
  assert.throws(() => loader.fetchBlock('F', '0099'), /no such block/);
  assert.throws(() => loader.fetchBlock('G', 'base'), /no such block/);
  assert.throws(() => loader.fetchBlock('F', '../../etc/passwd'), /no such block/);
  assert.equal(asked.length, 0);
});

test('without crypto.subtle the size alone is checked; the store keeps only what the manifest lists', async () => {
  const { subsets, files } = await build();
  const store = memoryStore();
  const loader = new SubsetLoader(subsets, env(files, [], { subtle: undefined, store }));
  await loader.fetchBlock('F', 'base');
  await loader.prune();
  assert.deepEqual(store.pruned[0].sort(), subsets.fonts[0].blocks.map((b) => b.sha256).sort());
});
