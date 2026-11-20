// The little of node's test runner these tests use (no @types/node in the project).
declare module 'node:test' {
  const test: (name: string, fn: () => void | Promise<void>) => void;
  export default test;
}
declare module 'node:assert/strict' {
  const assert: {
    equal(actual: unknown, expected: unknown, message?: string): void;
    deepEqual(actual: unknown, expected: unknown, message?: string): void;
    ok(value: unknown, message?: string): void;
    throws(fn: () => unknown, expected?: RegExp): void;
    rejects(promise: Promise<unknown>, expected?: RegExp | ((error: unknown) => boolean)): Promise<void>;
  };
  export default assert;
}
declare module 'node:vm' {
  export function runInNewContext(code: string): unknown;
}
