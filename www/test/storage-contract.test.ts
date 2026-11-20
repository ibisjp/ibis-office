// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// The contract test (storage-contract.ts) against a reference storage with the three kinds of roots, paged lists, a slow continuation, and a folder that
// cannot be written; and against broken ones, to show that it says so.
import test from 'node:test';
import assert from 'node:assert/strict';
import type { HostFolder, HostFolderListing, HostStorage } from '../ibisoffice-host.js';
import { verifyHostStorage } from './storage-contract.js';

interface Node {
  folder: HostFolder;
  children: string[];
  late: string[];
  files: string[];
}

/** A reference storage: shared root with 12 folders, a personal root, a joined root whose last two folders come after the continuation. */
function reference(options: { loopCursor?: boolean; noKind?: boolean } = {}): HostStorage {
  const nodes = new Map<string, Node>();
  const add = (id: string, name: string, canWrite: boolean, rootKind?: HostFolder['rootKind']): Node => {
    const node: Node = { folder: { id, name, hasSubFolders: false, canWrite, ...(rootKind && !options.noKind ? { rootKind } : {}) }, children: [], late: [], files: [] };
    nodes.set(id, node);
    return node;
  };
  const shared = add('s', 'Storage', true, 'shared');
  add('p', 'Mine', true, 'personal');
  const joined = add('joined', 'Joined', false, 'joined');
  for (let i = 0; i < 12; i++) {
    add(`s${i}`, `Folder ${i}`, i !== 3);
    shared.children.push(`s${i}`);
  }
  for (const name of ['own', 'other1', 'other2']) {
    add(`j-${name}`, name, true);
    (name === 'own' ? joined.children : joined.late).push(`j-${name}`);
  }
  const roots = ['s', 'p', 'joined'];
  return {
    listFolder: async (req) => {
      const parent = req.folderId === null ? undefined : nodes.get(req.folderId);
      if (req.folderId !== null && !parent) {
        return { ok: false, reason: 'notFound' };
      }
      const all = parent ? (req.cursor?.startsWith('late:') ? [...parent.children, ...parent.late] : parent.children) : roots;
      const base = req.cursor?.startsWith('late:') ? Number(req.cursor.slice(5)) : req.cursor ? Number(req.cursor) : 0;
      const slice = all.slice(base, base + req.limit);
      const end = base + slice.length;
      const quick = !req.cursor?.startsWith('late:');
      const answer: HostFolderListing = {
        folder: parent && req.folderId !== null ? { ...parent.folder, path: [{ id: roots.includes(req.folderId) ? req.folderId : 's', name: 'root' }] } : null,
        folders: slice.map((id) => nodes.get(id)!.folder),
        files: [],
      };
      if (end < all.length) {
        answer.nextCursor = options.loopCursor ? '0' : req.cursor?.startsWith('late:') ? `late:${end}` : String(end);
      }
      if (parent && parent.late.length > 0 && quick) {
        answer.deferredCursor = 'late:0';
      }
      return answer;
    },
  };
}

test('a storage that keeps the promises has no problem', async () => {
  assert.deepEqual(await verifyHostStorage(reference()), []);
});

test('the contract names a nextCursor that never ends', async () => {
  const problems = await verifyHostStorage(reference({ loopCursor: true }));
  assert.ok(problems.some((p) => p.includes('nextCursor repeats')), problems.join('\n'));
});

test('the contract names roots with no rootKind', async () => {
  const problems = await verifyHostStorage(reference({ noKind: true }));
  assert.ok(problems.some((p) => p.includes('has no rootKind')), problems.join('\n'));
});

test('a storage without listFolder is told what the smallest is', async () => {
  const problems = await verifyHostStorage({});
  assert.equal(problems.length, 1);
  assert.ok(/listFolder/.test(problems[0]));
});
