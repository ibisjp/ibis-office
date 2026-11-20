// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// The blocks of the fonts (GitHub issue #157, stage 2): `loader-manifest.json`'s `subsets` lists, for each font, the blocks `tools/ibis-font-subsetter` cut it into
// (a base block and blocks of the rest of the characters, each a font of its own). The page asks for a block by the font's id and the block's key, never for a
// character: what is sent to the server is the name of a block, not what a document says. A block is taken from the store of what was fetched before (IndexedDB,
// by the SHA-256 of the block), else fetched, checked (size and SHA-256) and kept; a block that cannot be had is not asked for again until `forget`. Tested in node
// (www/test/subset-fetch.test.ts).

/** One block of a font, as the manifest lists it. */
export interface SubsetBlock {
  key: string;
  /** The family name of the block's font (`ibis <id> <key>`): what the text system is asked for. */
  family: string;
  /** The path from the page's folder. */
  file: string;
  size: number;
  sha256: string;
  /** The code points the block has: ranges `[first, last]`. */
  ranges: [number, number][];
  /** Taken when the page starts. */
  boot?: boolean;
}

export interface SubsetFont {
  id: string;
  /** The typefaces a document names for this font (`BIZ UDゴシック`, ...). */
  typeface: string[];
  license: string;
  licenseFile: string;
  blocks: SubsetBlock[];
}

export interface Subsets {
  schema: number;
  fonts: SubsetFont[];
}

/** What was fetched before, kept by the block's SHA-256 (IndexedDB in the page). */
export interface BlockStore {
  get(sha256: string): Promise<Uint8Array | undefined>;
  put(sha256: string, bytes: Uint8Array): Promise<void>;
  /** Everything but `keep` is removed (the blocks of an older build). */
  prune?(keep: ReadonlySet<string>): Promise<void>;
}

export interface SubsetEnv {
  fetch: typeof fetch;
  /** Absent where the page is not a secure context: the size alone is checked then. */
  subtle?: Pick<SubtleCrypto, 'digest'>;
  base: URL;
  store?: BlockStore;
  warn: (message: string) => void;
  /** Retries of one block after a failure (default 3) and the wait before the first one, doubled each time (default 500 ms). */
  retries?: number;
  backoffMs?: number;
}

/** The subsets of a manifest, or null when it has none (or not ones this loader understands). */
export function subsetsOf(manifest: { subsets?: unknown } | null): Subsets | null {
  const subsets = manifest?.subsets as Subsets | undefined;
  if (!subsets || subsets.schema !== 1 || !Array.isArray(subsets.fonts)) {
    return null;
  }
  return subsets;
}

function hex(buffer: ArrayBuffer): string {
  return [...new Uint8Array(buffer)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

/** The key of the block of `font` that has `codePoint`, if any. */
export function blockOf(font: SubsetFont, codePoint: number): SubsetBlock | undefined {
  return font.blocks.find((b) => b.ranges.some(([first, last]) => first <= codePoint && codePoint <= last));
}

/** The font whose typeface list names `typeface` (the names are compared as they are written, ignoring case). */
export function fontFor(subsets: Subsets, typeface: string): SubsetFont | undefined {
  const wanted = typeface.trim().toLowerCase();
  return subsets.fonts.find((f) => f.typeface.some((t) => t.trim().toLowerCase() === wanted));
}

/** The loader of blocks. One per page. */
export class SubsetLoader {
  private readonly inflight = new Map<string, Promise<Uint8Array>>();
  private readonly failed = new Map<string, Error>();
  private readonly fetched = new Set<string>();

  constructor(
    readonly subsets: Subsets,
    private readonly env: SubsetEnv,
  ) {}

  private block(fontId: string, key: string): SubsetBlock {
    const font = this.subsets.fonts.find((f) => f.id === fontId);
    const block = font?.blocks.find((b) => b.key === key);
    // What the manifest does not list is not taken: a request for anything else is an error, never a fetch.
    if (!block) {
      throw new Error(`no such block: ${fontId}/${key}`);
    }
    return block;
  }

  /** The blocks the page takes when it starts (the base block of each font). */
  bootBlocks(): { fontId: string; key: string }[] {
    return this.subsets.fonts.flatMap((f) => f.blocks.filter((b) => b.boot).map((b) => ({ fontId: f.id, key: b.key })));
  }

  /** The bytes of a block. The same block asked for again while it is coming is one fetch; one that came is not fetched again. */
  fetchBlock(fontId: string, key: string): Promise<Uint8Array> {
    const id = `${fontId}/${key}`;
    const failure = this.failed.get(id);
    if (failure) {
      return Promise.reject(failure);
    }
    const running = this.inflight.get(id);
    if (running) {
      return running;
    }
    const promise = this.load(this.block(fontId, key))
      .then((bytes) => {
        this.fetched.add(id);
        return bytes;
      })
      .catch((error: Error) => {
        // Not tried again in this session (until `forget`): the same failure is not repeated for every character that needs the block.
        this.failed.set(id, error);
        throw error;
      })
      .finally(() => this.inflight.delete(id));
    this.inflight.set(id, promise);
    return promise;
  }

  /** The blocks that could not be had are tried again by the next request (back online, the user's retry). */
  forget(): void {
    this.failed.clear();
  }

  /** The keys of the blocks that were taken. */
  taken(): string[] {
    return [...this.fetched];
  }

  /** The store keeps what the manifest lists and nothing else (a new build's manifest does not list the old blocks). */
  async prune(): Promise<void> {
    const keep = new Set(this.subsets.fonts.flatMap((f) => f.blocks.map((b) => b.sha256.toLowerCase())));
    await this.env.store?.prune?.(keep);
  }

  private async verified(bytes: Uint8Array, block: SubsetBlock): Promise<boolean> {
    if (bytes.length !== block.size) {
      return false;
    }
    if (!this.env.subtle) {
      return true;
    }
    return hex(await this.env.subtle.digest('SHA-256', bytes as BufferSource)) === block.sha256.toLowerCase();
  }

  private async load(block: SubsetBlock): Promise<Uint8Array> {
    const kept = await this.env.store?.get(block.sha256.toLowerCase()).catch(() => undefined);
    if (kept && (await this.verified(kept, block))) {
      return kept;
    }
    const retries = this.env.retries ?? 3;
    const wait = this.env.backoffMs ?? 500;
    let last = new Error('not fetched');
    for (let attempt = 0; attempt <= retries; attempt++) {
      try {
        const response = await this.env.fetch(new URL(block.file, this.env.base));
        if (!response.ok) {
          throw new Error(`${block.file}: HTTP ${response.status}`);
        }
        const bytes = new Uint8Array(await response.arrayBuffer());
        if (!(await this.verified(bytes, block))) {
          throw new Error(`${block.file}: not the block of the manifest (its size or SHA-256)`);
        }
        await this.env.store?.put(block.sha256.toLowerCase(), bytes).catch(() => undefined);
        return bytes;
      } catch (error) {
        last = error as Error;
        if (attempt < retries) {
          await new Promise((resolve) => setTimeout(resolve, wait * 2 ** attempt));
        }
      }
    }
    this.env.warn(`${block.file}: ${last.message}`);
    throw last;
  }
}
