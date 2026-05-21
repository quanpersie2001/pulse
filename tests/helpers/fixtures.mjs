import path from "node:path";
import { fileURLToPath } from "node:url";

export const TESTS_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const REPO_ROOT = path.resolve(TESTS_ROOT, "..");
export const FIXTURES_ROOT = path.join(TESTS_ROOT, "fixtures");

export function fixturePath(...segments) {
  return path.join(FIXTURES_ROOT, ...segments);
}
