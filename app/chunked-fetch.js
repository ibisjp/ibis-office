// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// The wasm in pieces (GitHub issue #152): a CDN with a limit on the size of a file serves the wasm as parts that `scripts/web-loader-setup.mjs` made, and
// `loader-manifest.json` lists them. The parts are fetched in parallel (4 at a time), each one is checked (size, and SHA-256 of the bytes as stored when there are two or more) before
// it is used, and they are handed over in order as a stream, so that the browser compiles the wasm while it is still arriving. Without a manifest the
// wasm is the one file it always was (`loadChunkedWasm` answers null and the caller fetches it). Tested in node (www/test/chunked-fetch.test.ts).
export const MANIFEST_NAME = 'loader-manifest.json';
/** The failure of one part (which one and why), as the status line shows it. */
export class ChunkError extends Error {
    chunk;
    constructor(chunk, reason) {
        super(`${chunk}: ${reason}`);
        this.chunk = chunk;
        this.name = 'ChunkError';
    }
}
function hex(buffer) {
    return [...new Uint8Array(buffer)].map((b) => b.toString(16).padStart(2, '0')).join('');
}
/** The manifest, or null when there is none (a 404, or no way to ask for it at all). A manifest that is not understood is an error. */
export async function readManifest(env) {
    const url = new URL(MANIFEST_NAME, env.base);
    let response;
    try {
        response = await env.fetch(url, { cache: 'no-cache' });
    }
    catch {
        // Offline: the manifest of the last visit (the HTTP cache), else the one file as before.
        try {
            response = await env.fetch(url, { cache: 'force-cache' });
        }
        catch {
            return null;
        }
    }
    if (response.status === 404) {
        return null;
    }
    if (!response.ok) {
        env.warn(`${MANIFEST_NAME}: HTTP ${response.status}; the wasm is fetched as one file`);
        return null;
    }
    let manifest;
    try {
        manifest = (await response.json());
    }
    catch {
        throw new Error(`${MANIFEST_NAME} is not JSON`);
    }
    if (manifest?.schema !== 1 || !Array.isArray(manifest.files)) {
        throw new Error(`${MANIFEST_NAME}: not a manifest this loader understands (schema ${String(manifest?.schema)})`);
    }
    if (manifest.app !== env.appVersion) {
        throw new Error(`${MANIFEST_NAME} is of version ${String(manifest.app)}, this page is ${env.appVersion}: the files are of two builds`);
    }
    return manifest;
}
/** The table of the design (2.3): the argument against the manifest. */
function compareArgument(env, manifest) {
    const arg = env.chunkArg;
    if (arg === undefined || !Number.isFinite(arg)) {
        return;
    }
    if (manifest === null) {
        if (arg > 0) {
            env.warn(`chunk=${arg} MiB was given but there is no ${MANIFEST_NAME}: the wasm is fetched as one file`);
        }
        return;
    }
    const manifestMiB = manifest.chunkSize / (1024 * 1024);
    if (arg === 0) {
        env.warn(`chunk=0 (no parts) was given but ${MANIFEST_NAME} lists parts: the manifest is used`);
    }
    else if (Math.abs(arg - manifestMiB) > 1e-9) {
        env.warn(`chunk=${arg} MiB was given, the manifest says ${manifestMiB} MiB: the manifest is used`);
    }
}
/**
 * The wasm as a Response made of the parts of the manifest, or null when there is no manifest (the caller fetches the one file).
 * A part that fails its size or hash is fetched again (3 times); one that keeps failing stops the start with the part named. Nothing that failed
 * is handed to the compiler.
 */
export async function loadChunkedWasm(env, read) {
    const manifest = read ? read.manifest : await readManifest(env);
    compareArgument(env, manifest);
    if (manifest === null) {
        return null;
    }
    const file = manifest.files.find((f) => f.role === 'wasm');
    if (!file && manifest.files.length === 0 && manifest.subsets) {
        return null; // a manifest of fonts only: the wasm is the one file
    }
    if (!file || !Array.isArray(file.chunks) || file.chunks.length === 0) {
        throw new Error(`${MANIFEST_NAME}: the wasm is not listed`);
    }
    const chunks = file.chunks;
    const total = chunks.reduce((sum, c) => sum + c.size, 0);
    const concurrency = Math.max(1, env.concurrency ?? 4);
    const retries = env.retries ?? 3;
    const backoff = env.backoffMs ?? 500;
    // A wasm in one part is not hashed (the user, 2026-11-17): nothing can be mixed from two builds, and a damaged download fails in the compiler.
    // Its size is still checked. Two parts or more are hashed one by one.
    const hashParts = chunks.length > 1;
    if (hashParts && !env.subtle) {
        env.warn('crypto.subtle is not available on this page (not a secure context): the parts are checked by size only');
    }
    let loaded = 0;
    const fetchOnce = async (chunk) => {
        let response;
        try {
            response = await env.fetch(new URL(chunk.name, env.base));
        }
        catch (error) {
            throw new ChunkError(chunk.name, `could not be fetched (${String(error)})`);
        }
        if (!response.ok) {
            throw new ChunkError(chunk.name, `HTTP ${response.status}`);
        }
        const bytes = new Uint8Array(await response.arrayBuffer());
        if (bytes.length !== chunk.size) {
            throw new ChunkError(chunk.name, `${bytes.length} bytes, the manifest says ${chunk.size}`);
        }
        if (hashParts && env.subtle) {
            const digest = hex(await env.subtle.digest('SHA-256', bytes));
            if (digest !== chunk.sha256.toLowerCase()) {
                throw new ChunkError(chunk.name, 'its SHA-256 differs from the manifest (a damaged part, or a part of another build)');
            }
        }
        return bytes;
    };
    const fetchChunk = async (chunk) => {
        for (let attempt = 0;; attempt++) {
            try {
                const bytes = await fetchOnce(chunk);
                loaded += bytes.length;
                env.onProgress?.(loaded, total);
                return bytes;
            }
            catch (error) {
                if (attempt >= retries) {
                    throw error;
                }
                await new Promise((resolve) => setTimeout(resolve, backoff * 2 ** attempt));
            }
        }
    };
    // The parts start in order, at most `concurrency` ahead of the one the stream is waiting for (the memory stays at a few parts).
    const results = [];
    let consumed = 0;
    const start = () => {
        while (results.length < chunks.length && results.length < consumed + concurrency) {
            const promise = fetchChunk(chunks[results.length]);
            promise.catch(() => undefined); // The failure is raised where the stream reaches the part.
            results.push(promise);
        }
    };
    start();
    const source = new ReadableStream({
        async pull(controller) {
            if (consumed >= chunks.length) {
                controller.close();
                return;
            }
            start();
            const bytes = await results[consumed];
            consumed += 1;
            controller.enqueue(bytes);
            start();
        },
    }, { highWaterMark: 0 });
    let body = source;
    if (manifest.encoding === 'gzip') {
        if (!env.decompress) {
            throw new Error('this browser cannot expand the wasm (DecompressionStream is missing)');
        }
        body = env.decompress(source);
    }
    return new Response(body, { headers: { 'Content-Type': file.contentType ?? 'application/wasm' } });
}
