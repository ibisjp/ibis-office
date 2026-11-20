// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// The loader of the wasm in parts (chunked-fetch.ts, GitHub issue #152), in node with a fake server: no manifest, the table of the argument against the
// manifest, a damaged part, a part that is missing, parts swapped, a manifest of another build, no crypto.subtle, and the progress.
import test from 'node:test';
import assert from 'node:assert/strict';
import { loadChunkedWasm, MANIFEST_NAME, type ChunkedEnv, type Manifest } from '../chunked-fetch.js';

const BASE = new URL('https://example.com/app/');
/** SHA-256 (hex) as the loader computes it, so a test's expectation is the same function (the tool's own is tested in scripts/tests). */
const sha = async (b: Uint8Array): Promise<string> => [...new Uint8Array(await crypto.subtle.digest('SHA-256', b as BufferSource))].map((x) => x.toString(16).padStart(2, '0')).join('');

const same = (a: Uint8Array, b: Uint8Array): boolean => a.length === b.length && a.every((v, i) => v === b[i]);

async function gzip(bytes: Uint8Array): Promise<Uint8Array> {
  const stream = new Blob([bytes as BlobPart]).stream().pipeThrough(new CompressionStream('gzip'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** A wasm of `size` bytes, stored (gzip or none) and cut into parts of `part` bytes, with the manifest and a server of them. */
async function build(size: number, part: number, encoding: 'gzip' | 'none', app = '9.9.9') {
  const whole = Uint8Array.from({ length: size }, (_, i) => (i * 31 + (i >> 3)) & 255);
  const stored = encoding === 'gzip' ? await gzip(whole) : whole;
  const parts: Uint8Array[] = [];
  for (let at = 0; at < stored.length; at += part) {
    parts.push(stored.slice(at, at + part));
  }
  const manifest: Manifest = {
    schema: 1,
    app,
    chunkSize: part,
    encoding,
    files: [
      {
        path: 'pkg/ibis_office_app_bg.wasm',
        role: 'wasm',
        contentType: 'application/wasm',
        size,
        sha256: await sha(whole),
        chunks: await Promise.all(parts.map(async (p, i) => ({ name: `pkg/w.${String(i).padStart(3, '0')}.part`, size: p.length, sha256: await sha(p) }))),
      },
    ],
  };
  const files = new Map<string, Uint8Array>(parts.map((p, i) => [`https://example.com/app/${manifest.files[0].chunks[i].name}`, p]));
  return { whole, manifest, files, parts };
}

interface Server {
  files: Map<string, Uint8Array>;
  manifest?: unknown;
  /** URLs asked, in order. */
  asked: string[];
  /** Fail the next n asks of this URL (a network error). */
  flaky: Map<string, number>;
}

function server(files: Map<string, Uint8Array>, manifest?: unknown): Server {
  return { files, manifest, asked: [], flaky: new Map() };
}

function env(s: Server, over: Partial<ChunkedEnv> = {}, warnings: string[] = []): ChunkedEnv {
  return {
    fetch: (async (input: RequestInfo | URL) => {
      const url = String(input);
      s.asked.push(url);
      const left = s.flaky.get(url) ?? 0;
      if (left > 0) {
        s.flaky.set(url, left - 1);
        throw new Error('network down');
      }
      if (url === new URL(MANIFEST_NAME, BASE).href) {
        return s.manifest === undefined ? new Response('', { status: 404 }) : new Response(JSON.stringify(s.manifest));
      }
      const bytes = s.files.get(url);
      return bytes ? new Response(bytes as BodyInit) : new Response('', { status: 404 });
    }) as typeof fetch,
    subtle: globalThis.crypto.subtle,
    appVersion: '9.9.9',
    base: BASE,
    warn: (m) => void warnings.push(m),
    decompress: (source) => source.pipeThrough(new DecompressionStream('gzip') as unknown as ReadableWritablePair<Uint8Array, Uint8Array>),
    backoffMs: 1,
    ...over,
  };
}

async function bytesOf(response: Response): Promise<Uint8Array> {
  return new Uint8Array(await response.arrayBuffer());
}

test('without a manifest the wasm is the one file (null), quietly', async () => {
  const warnings: string[] = [];
  assert.equal(await loadChunkedWasm(env(server(new Map()), {}, warnings)), null);
  assert.deepEqual(warnings, []);
});

test('with a manifest the parts are joined in order, expanded (gzip or not), and give the file', async () => {
  for (const encoding of ['gzip', 'none'] as const) {
    const built = await build(100_000, 7000, encoding);
    const progress: number[] = [];
    const response = await loadChunkedWasm(env(server(built.files, built.manifest), { onProgress: (loaded) => void progress.push(loaded) }));
    assert.equal(response?.headers.get('Content-Type'), 'application/wasm');
    assert.ok(same(await bytesOf(response!), built.whole), encoding);
    const stored = built.parts.reduce((n, p) => n + p.length, 0);
    assert.equal(progress.at(-1), stored, 'the progress ends at the sum of the parts of the manifest');
    assert.ok(progress.every((v, i) => i === 0 || v > progress[i - 1]));
  }
});

test('at most 4 parts are asked for ahead of the one that is waited for', async () => {
  const built = await build(60_000, 1000, 'none');
  const s = server(built.files, built.manifest);
  let inflight = 0;
  let peak = 0;
  const e = env(s);
  const inner = e.fetch;
  e.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    inflight += 1;
    peak = Math.max(peak, inflight);
    await new Promise((r) => setTimeout(r, 2));
    inflight -= 1;
    return inner(input, init);
  }) as typeof fetch;
  await bytesOf((await loadChunkedWasm(e))!);
  assert.ok(peak <= 4 + 1, `peak ${peak}`); // the manifest or the next part may overlap the last one
});

test('the argument against the manifest (design 2.3): the manifest is the truth, and the difference is said', async () => {
  const built = await build(10_000, 1024 * 1024, 'none'); // chunkSize 1 MiB in the manifest
  built.manifest.chunkSize = 1024 * 1024;
  const cases: [number | undefined, RegExp | null][] = [
    [undefined, null],
    [1, null],
    [20, /the manifest says 1 MiB/],
    [0, /chunk=0/],
  ];
  for (const [arg, expected] of cases) {
    const warnings: string[] = [];
    const response = await loadChunkedWasm(env(server(built.files, built.manifest), { chunkArg: arg }, warnings));
    assert.ok(response, `argument ${String(arg)}: the manifest is used`);
    if (expected) {
      assert.ok(warnings.some((w) => expected.test(w)), `${String(arg)}: ${warnings.join('|')}`);
    } else {
      assert.deepEqual(warnings, []);
    }
  }
  // An argument and no manifest: the one file, with a warning.
  const warnings: string[] = [];
  assert.equal(await loadChunkedWasm(env(server(new Map()), { chunkArg: 20 }, warnings)), null);
  assert.ok(warnings.some((w) => /no loader-manifest/.test(w)));
});

test('a damaged part is fetched again; the damaged bytes never reach the reader', async () => {
  const built = await build(20_000, 4000, 'none');
  const url = `https://example.com/app/${built.manifest.files[0].chunks[2].name}`;
  const good = built.files.get(url)!;
  let served = 0;
  const s = server(built.files, built.manifest);
  const e = env(s);
  const inner = e.fetch;
  e.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    if (String(input) === url && served++ === 0) {
      const bad = good.slice();
      bad[10] ^= 1;
      return new Response(bad as BodyInit);
    }
    return inner(input, init);
  }) as typeof fetch;
  const result = await bytesOf((await loadChunkedWasm(e))!);
  assert.ok(same(result, built.whole));
  assert.equal(served, 2, 'only that part was fetched again');
});

test('a part that keeps failing stops the start, naming it; what came before is whole, nothing damaged follows', async () => {
  const built = await build(20_000, 4000, 'none');
  const name = built.manifest.files[0].chunks[3].name;
  const damaged = built.files.get(`https://example.com/app/${name}`)!.slice();
  damaged[0] ^= 255;
  built.files.set(`https://example.com/app/${name}`, damaged);
  const response = await loadChunkedWasm(env(server(built.files, built.manifest)));
  await assert.rejects(bytesOf(response!), (error: unknown) => (error as Error).message.includes(name) && /SHA-256/.test((error as Error).message));
});

test('a missing part (404), a part of the wrong size, and parts swapped are all refused', async () => {
  const built = await build(20_000, 4000, 'none');
  const urlOf = (i: number): string => `https://example.com/app/${built.manifest.files[0].chunks[i].name}`;
  const missing = new Map(built.files);
  missing.delete(urlOf(1));
  await assert.rejects((async () => bytesOf((await loadChunkedWasm(env(server(missing, built.manifest), { retries: 1 })))!))(), /HTTP 404/);
  const short = new Map(built.files);
  short.set(urlOf(1), built.files.get(urlOf(1))!.slice(0, 100));
  await assert.rejects((async () => bytesOf((await loadChunkedWasm(env(server(short, built.manifest), { retries: 0 })))!))(), /bytes, the manifest says/);
  const swapped = new Map(built.files);
  swapped.set(urlOf(0), built.files.get(urlOf(1))!);
  swapped.set(urlOf(1), built.files.get(urlOf(0))!);
  await assert.rejects((async () => bytesOf((await loadChunkedWasm(env(server(swapped, built.manifest), { retries: 0 })))!))(), /SHA-256/);
});

test('a network error on a part is fetched again with a wait, up to 3 times', async () => {
  const built = await build(10_000, 4000, 'none');
  const s = server(built.files, built.manifest);
  s.flaky.set(`https://example.com/app/${built.manifest.files[0].chunks[1].name}`, 3);
  assert.ok(same(await bytesOf((await loadChunkedWasm(env(s)))!), built.whole));
  s.flaky.set(`https://example.com/app/${built.manifest.files[0].chunks[1].name}`, 4);
  await assert.rejects((async () => bytesOf((await loadChunkedWasm(env(s)))!))(), /could not be fetched/);
});

test('a manifest of another build or another schema is not used (the start fails)', async () => {
  const built = await build(1000, 500, 'none', '9.9.8');
  await assert.rejects(loadChunkedWasm(env(server(built.files, built.manifest))), /two builds/);
  const other = { ...(await build(1000, 500, 'none')).manifest, schema: 2 };
  await assert.rejects(loadChunkedWasm(env(server(new Map(), other))), /schema 2/);
  await assert.rejects(loadChunkedWasm(env(server(new Map(), { schema: 1, app: '9.9.9', files: [] }))), /not listed/);
});

test('without crypto.subtle the parts are checked by size only, with a warning', async () => {
  const built = await build(10_000, 4000, 'none');
  const warnings: string[] = [];
  const corrupted = new Map(built.files);
  const url = `https://example.com/app/${built.manifest.files[0].chunks[0].name}`;
  const bad = built.files.get(url)!.slice();
  bad[1] ^= 1;
  corrupted.set(url, bad);
  const response = await loadChunkedWasm(env(server(corrupted, built.manifest), { subtle: undefined }, warnings));
  assert.equal((await bytesOf(response!)).length, 10_000, 'the size is checked, the content is not');
  assert.ok(warnings.some((w) => /crypto\.subtle/.test(w)));
});

test('offline: the manifest of the last visit (the cache) is used; with none, the one file', async () => {
  const built = await build(5000, 2000, 'none');
  const s = server(built.files, built.manifest);
  const manifestUrl = new URL(MANIFEST_NAME, BASE).href;
  const modes: string[] = [];
  const e = env(s);
  const inner = e.fetch;
  e.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    if (String(input) === manifestUrl) {
      modes.push(String(init?.cache));
      if (init?.cache === 'no-cache') {
        throw new Error('offline');
      }
    }
    return inner(input, init);
  }) as typeof fetch;
  assert.ok(await loadChunkedWasm(e));
  assert.deepEqual(modes, ['no-cache', 'force-cache']);
  const none = env(server(new Map()));
  const innerNone = none.fetch;
  none.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    if (String(input) === manifestUrl) {
      throw new Error('offline');
    }
    return innerNone(input, init);
  }) as typeof fetch;
  assert.equal(await loadChunkedWasm(none), null);
});

test('a wasm in one part is not hashed (only its size is checked), and gives no warning without crypto.subtle', async () => {
  const built = await build(5000, 8000, 'none');
  assert.equal(built.manifest.files[0].chunks.length, 1);
  const s = server(built.files, built.manifest);
  let digests = 0;
  const warnings: string[] = [];
  const subtle = { digest: async (a: AlgorithmIdentifier, d: BufferSource) => { digests++; return crypto.subtle.digest(a, d); } };
  const result = await bytesOf((await loadChunkedWasm(env(s, { subtle }, warnings)))!);
  assert.ok(same(result, built.whole));
  assert.equal(digests, 0, 'no SHA-256 for one part');
  const quiet: string[] = [];
  await bytesOf((await loadChunkedWasm(env(server(built.files, built.manifest), { subtle: undefined }, quiet)))!);
  assert.deepEqual(quiet.filter((w) => w.includes('crypto.subtle')), []);
});
