// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// The contract of a storage SaaS (`HostStorage`, K0041) as a test: give it an adapter and it says which of the promises are broken (an empty list: none).
// What it looks at: the roots of listFolder(null), the ids and cursors being strings of at most 255 characters, a page of at most `limit`, the next pages
// by nextCursor reaching an end without a repeat, the path starting with the root the folder belongs to, canWrite, and the slow continuation
// (deferredCursor) that gives its own list. A SaaS copies this file into its tests and passes its adapter.
import type { HostFailure, HostFolder, HostFolderListing, HostStorage } from '../ibisoffice-host.js';

const MAX_ID = 255;
const PAGE = 5;
const MAX_PAGES = 1000;

const failed = (v: unknown): v is HostFailure => (v as HostFailure | null)?.ok === false;

/** Every page of a folder through nextCursor; the broken promises are put in `problems`. */
async function allPages(storage: HostStorage, folderId: string | null, problems: string[], cursor?: string): Promise<{ folders: HostFolder[]; files: string[]; deferred: string | undefined; first?: HostFolderListing }> {
  const folders: HostFolder[] = [];
  const files: string[] = [];
  let deferred: string | undefined;
  let first: HostFolderListing | undefined;
  const seen = new Set<string>();
  for (let page = 0; page < MAX_PAGES; page++) {
    const answer = await storage.listFolder!({ folderId, purpose: 'open', include: { folders: true, files: true }, cursor, limit: PAGE });
    if (failed(answer)) {
      problems.push(`listFolder(${String(folderId)}) failed: ${answer.reason}`);
      break;
    }
    first ??= answer;
    if (answer.folders.length + answer.files.length > PAGE) {
      problems.push(`a page of listFolder(${String(folderId)}) is longer than limit (${PAGE})`);
    }
    for (const f of answer.folders) {
      folders.push(f);
    }
    for (const f of answer.files) {
      files.push(f.id);
    }
    deferred ??= answer.deferredCursor;
    if (answer.nextCursor === undefined) {
      return { folders, files, deferred, first };
    }
    if (typeof answer.nextCursor !== 'string' || answer.nextCursor.length > MAX_ID) {
      problems.push('nextCursor is not a string of 255 characters or less');
      break;
    }
    if (seen.has(answer.nextCursor)) {
      problems.push(`nextCursor repeats (${answer.nextCursor}): the pages do not end`);
      break;
    }
    seen.add(answer.nextCursor);
    cursor = answer.nextCursor;
  }
  return { folders, files, deferred, first };
}

/** Looks at an adapter against the promises of `HostStorage` (design 4.8). */
export async function verifyHostStorage(storage: HostStorage): Promise<string[]> {
  const problems: string[] = [];
  if (typeof storage.listFolder !== 'function') {
    return ['listFolder is not given (the smallest storage has listFolder and saveFileAs)'];
  }
  const top = await allPages(storage, null, problems);
  if (top.folders.length === 0) {
    problems.push('listFolder(null) has no root');
  }
  if (top.first && top.first.folder !== null) {
    problems.push('listFolder(null): folder is not null');
  }
  if (top.files.length > 0) {
    problems.push('listFolder(null): the top has files (only roots)');
  }
  const ids = new Set<string>();
  for (const root of top.folders) {
    if (typeof root.id !== 'string' || root.id.length === 0 || root.id.length > MAX_ID) {
      problems.push(`a root has an id that is not a string of 1 to ${MAX_ID} characters`);
    }
    if (ids.has(root.id)) {
      problems.push(`a root repeats: ${root.id}`);
    }
    ids.add(root.id);
    if (root.rootKind === undefined) {
      problems.push(`the root ${root.id} has no rootKind`);
    }
    // The children of a root: through the pages, with no repeat, the path of the folder seen starting at the root (a root with no real folder: none).
    const inside = await allPages(storage, root.id, problems);
    const seenIds = new Set<string>();
    for (const f of inside.folders) {
      if (typeof f.id !== 'string' || f.id.length > MAX_ID) {
        problems.push(`a folder under ${root.id} has a bad id`);
      }
      if (seenIds.has(f.id)) {
        problems.push(`a folder repeats in the pages of ${root.id}: ${f.id}`);
      }
      seenIds.add(f.id);
      if (f.rootKind !== undefined) {
        problems.push(`the folder ${f.id} under ${root.id} has a rootKind (only roots have)`);
      }
    }
    const path = inside.first?.folder?.path;
    if (path && path.length > 0 && path[0].id !== root.id && root.rootKind === 'shared') {
      problems.push(`the path of ${root.id} does not start with the root`);
    }
    if (inside.deferred !== undefined) {
      // The slow continuation: it has its own list, and it ends.
      const more = await allPages(storage, root.id, problems, inside.deferred);
      const before = new Set(inside.folders.map((f) => f.id));
      if (more.folders.length === 0 && more.files.length === 0 && before.size === 0) {
        problems.push(`the deferredCursor of ${root.id} gives nothing`);
      }
      if (more.deferred !== undefined) {
        problems.push(`the deferredCursor of ${root.id} leads to another deferredCursor`);
      }
    }
    if (root.canWrite && inside.first?.folder && !inside.first.folder.canWrite) {
      problems.push(`${root.id}: canWrite differs between the list of the top and the folder itself`);
    }
  }
  return problems;
}
