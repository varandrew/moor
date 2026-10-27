import { execSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "..");

function run(cmd) {
  console.log(`> ${cmd}`);
  execSync(cmd, { stdio: "inherit", cwd: root });
}

function getRootVersion() {
  return JSON.parse(readFileSync(path.join(root, "package.json"), "utf8")).version;
}

// 检测 pre-release 模式并提示
const preJsonPath = path.join(root, ".changeset", "pre.json");
if (existsSync(preJsonPath)) {
  const pre = JSON.parse(readFileSync(preJsonPath, "utf8"));
  if (pre.mode === "pre") {
    console.log(`📦 Pre-release 模式: ${pre.tag}\n`);
  }
}

// 检查是否有待消费的 changeset 文件（直接扫描目录，不依赖 git status）
const changesetDir = path.join(root, ".changeset");
const pendingChangesets = readdirSync(changesetDir).filter(
  (f) => f.endsWith(".md") && f !== "README.md",
);

if (pendingChangesets.length === 0) {
  console.error("No pending changesets found. Run 'pnpm changeset' first.");
  process.exit(1);
}

console.log(
  `Found ${pendingChangesets.length} pending changeset(s): ${pendingChangesets.join(", ")}\n`,
);

// 检查工作区是否干净（忽略 .changeset/ 目录）
const status = execSync("git status --porcelain", { cwd: root, encoding: "utf8" }).trim();
if (status) {
  const dirtyFiles = status
    .split("\n")
    .filter((l) => !l.includes(".changeset"))
    .map((l) => l.trim())
    .filter(Boolean);
  if (dirtyFiles.length > 0) {
    console.error("Working tree has uncommitted changes outside .changeset/:");
    dirtyFiles.forEach((l) => console.error(`  ${l}`));
    console.error("\nCommit or stash these changes before releasing.");
    process.exit(1);
  }
}

run("pnpm changeset version");

// changeset writes CHANGELOG.md for the root package (moor) at the repo root.
run("pnpm version:sync");
run(
  "pnpm exec vp fmt CHANGELOG.md package.json packages/types/package.json src-tauri/tauri.conf.json",
);

const version = getRootVersion();
const tagName = `v${version}`;

// 检查 tag 是否已存在
const existingTags = execSync("git tag -l", { cwd: root, encoding: "utf8" }).trim().split("\n");
if (existingTags.includes(tagName)) {
  console.error(`\n❌ Tag "${tagName}" already exists.`);
  console.error("This usually means changeset did not bump the version as expected.");
  console.error(
    "Check .changeset/config.json and ensure the changeset file targets the correct package.",
  );
  console.error("\nRolling back the release commit...");
  run("git reset --soft HEAD~1");
  run("git checkout -- .");
  process.exit(1);
}

run(`git add -A`);
run(`git commit -m "chore(release): ${tagName}"`);
run(`git tag -a ${tagName} -m "${tagName}"`);

console.log(`\nRelease ${tagName} committed and tagged.`);
console.log(`Push to publish: git push origin main refs/tags/${tagName}:refs/tags/${tagName}`);
