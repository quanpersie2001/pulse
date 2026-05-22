import fs from "node:fs";
import path from "node:path";

import {
  copyPathIfExists,
  ensureDirectory,
  listDirectoryEntries,
} from "../core/fs.mjs";
import { relativePosix } from "../core/paths.mjs";

function utcNow() {
  return new Date().toISOString();
}

function isBackupEntry(name) {
  return /^backup-/.test(name);
}

function backupStamp() {
  return utcNow().replace(/[:.]/g, "-");
}

function listActiveDomainEntries(domainPath) {
  return listDirectoryEntries(domainPath).filter((entry) => !isBackupEntry(entry));
}

function classifyPulseDomain(repoRoot) {
  const pulsePath = path.join(repoRoot, ".pulse");
  if (!fs.existsSync(pulsePath)) {
    return { status: "missing", missing: [".pulse"], unexpected_entries: [], conflicts: [] };
  }

  const required = [
    ["runtime", "directory"],
    [path.join("runtime", "handoffs"), "directory"],
    ["workgraph", "directory"],
    [path.join("workgraph", "views"), "directory"],
    ["harness", "directory"],
    ["memory", "directory"],
    [path.join("harness", "HARNESS_BACKLOG.md"), "file"],
  ];
  const missing = [];
  for (const [relative, kind] of required) {
    const absolute = path.join(pulsePath, relative);
    const exists = fs.existsSync(absolute);
    if (!exists || (kind === "directory" && !fs.statSync(absolute).isDirectory()) || (kind === "file" && !fs.statSync(absolute).isFile())) {
      missing.push(path.posix.join(".pulse", relative.split(path.sep).join(path.posix.sep)));
    }
  }

  const allowedTopLevel = new Set(["runtime", "workgraph", "harness", "memory", "scripts"]);
  const unexpectedEntries = listActiveDomainEntries(pulsePath)
    .filter((entry) => !allowedTopLevel.has(entry))
    .map((entry) => path.posix.join(".pulse", entry));

  return {
    status: missing.length === 0 && unexpectedEntries.length === 0 ? "compliant" : "non_compliant",
    missing,
    unexpected_entries: unexpectedEntries,
    conflicts: [],
  };
}

function classifyDocsDomain(repoRoot) {
  const docsPath = path.join(repoRoot, "docs");
  if (!fs.existsSync(docsPath)) {
    return { status: "missing", missing: ["docs"], unexpected_entries: [], conflicts: [] };
  }
  const required = ["ARCHITECTURE.md", "GLOSSARY.md", "decisions", "product"];
  const missing = required.filter((entry) => !fs.existsSync(path.join(docsPath, entry)));
  return {
    status: missing.length === 0 ? "compliant" : "non_compliant",
    missing: missing.map((entry) => path.posix.join("docs", entry)),
    unexpected_entries: [],
    conflicts: [],
  };
}

function classifyWorksDomain(repoRoot) {
  const worksPath = path.join(repoRoot, "works");
  if (!fs.existsSync(worksPath)) {
    return { status: "missing", missing: ["works"], unexpected_entries: [], conflicts: [] };
  }

  const activeEntries = listActiveDomainEntries(worksPath);
  const allowedTopLevel = new Set(["epics", "backlog.md", "test-matrix.md"]);
  const unexpectedEntries = activeEntries.filter((entry) => !allowedTopLevel.has(entry));
  const missing = fs.existsSync(path.join(worksPath, "epics")) ? [] : ["works/epics"];

  return {
    status: missing.length === 0 && unexpectedEntries.length === 0 ? "compliant" : "non_compliant",
    missing,
    unexpected_entries: unexpectedEntries.map((entry) => path.posix.join("works", entry)),
    conflicts: [],
  };
}

export function classifyDomains(repoRoot) {
  return {
    pulse: classifyPulseDomain(repoRoot),
    docs: classifyDocsDomain(repoRoot),
    works: classifyWorksDomain(repoRoot),
  };
}

export function domainStatusSummary(domains) {
  return Object.fromEntries(Object.entries(domains).map(([name, value]) => [name, value.status]));
}

function backupDomainInPlace(repoRoot, relativePath, stamp) {
  const domainPath = path.join(repoRoot, relativePath);
  if (!fs.existsSync(domainPath)) {
    return { backup: "", moved: [] };
  }
  ensureDirectory(domainPath);
  const backupName = `backup-${stamp}`;
  const backupAbsolute = path.join(domainPath, backupName);
  const moved = [];
  ensureDirectory(backupAbsolute);
  for (const entry of fs.readdirSync(domainPath)) {
    if (entry === backupName || isBackupEntry(entry)) {
      continue;
    }
    fs.renameSync(path.join(domainPath, entry), path.join(backupAbsolute, entry));
    moved.push(entry);
  }
  return {
    backup: path.posix.join(relativePath.split(path.sep).join(path.posix.sep), backupName),
    moved,
  };
}

function ensurePulseDomainLayout(repoRoot) {
  for (const relative of [
    [".pulse", "runtime"],
    [".pulse", "runtime", "handoffs"],
    [".pulse", "runtime", "onboarding"],
    [".pulse", "workgraph"],
    [".pulse", "workgraph", "views"],
    [".pulse", "harness"],
    [".pulse", "memory"],
  ]) {
    ensureDirectory(path.join(repoRoot, ...relative));
  }

  const manifestPath = path.join(repoRoot, ".pulse", "runtime", "handoffs", "manifest.json");
  if (!fs.existsSync(manifestPath)) {
    fs.writeFileSync(manifestPath, `${JSON.stringify({ schema_version: "1.0", updated_at: utcNow(), active: [] }, null, 2)}\n`, "utf8");
  }
  const reservationsPath = path.join(repoRoot, ".pulse", "runtime", "reservations.json");
  if (!fs.existsSync(reservationsPath)) {
    fs.writeFileSync(reservationsPath, `${JSON.stringify({ schema_version: "1.0", reservations: [] }, null, 2)}\n`, "utf8");
  }
}

function ensureDocsScaffold(repoRoot) {
  ensureDirectory(path.join(repoRoot, "docs"));
  ensureDirectory(path.join(repoRoot, "docs", "decisions"));
  ensureDirectory(path.join(repoRoot, "docs", "product"));
  const architecturePath = path.join(repoRoot, "docs", "ARCHITECTURE.md");
  if (!fs.existsSync(architecturePath)) {
    fs.writeFileSync(architecturePath, "# Architecture\n", "utf8");
  }
  const glossaryPath = path.join(repoRoot, "docs", "GLOSSARY.md");
  if (!fs.existsSync(glossaryPath)) {
    fs.writeFileSync(glossaryPath, "# Glossary\n", "utf8");
  }
}

function ensureWorksScaffold(repoRoot) {
  ensureDirectory(path.join(repoRoot, "works"));
  ensureDirectory(path.join(repoRoot, "works", "epics"));
}

function restorePulseBackup(repoRoot, backupRelativePath) {
  const restored = [];
  const notes = [];
  if (!backupRelativePath) {
    return { restored, notes };
  }

  const backupAbsolute = path.join(repoRoot, backupRelativePath);
  const copyIfPresent = (from, to) => {
    if (copyPathIfExists(path.join(backupAbsolute, from), path.join(repoRoot, to))) {
      restored.push(`${path.posix.join(backupRelativePath, from.split(path.sep).join(path.posix.sep))} -> ${to.split(path.sep).join(path.posix.sep)}`);
    }
  };

  copyIfPresent("runtime", path.join(".pulse", "runtime"));
  copyIfPresent("memory", path.join(".pulse", "memory"));
  copyIfPresent("workgraph", path.join(".pulse", "workgraph"));

  const unmapped = listActiveDomainEntries(backupAbsolute).filter(
    (entry) => !["runtime", "memory", "workgraph"].includes(entry),
  );
  if (unmapped.length > 0) {
    notes.push(`Unmapped .pulse backup entries require review: ${unmapped.join(", ")}.`);
  }

  return { restored, notes };
}

function writeOnboardingReconstructionBriefs(repoRoot, normalization) {
  const reconstructionDir = path.join(repoRoot, ".pulse", "runtime", "onboarding");
  ensureDirectory(reconstructionDir);
  const briefs = [];

  const writeBrief = (fileName, lines) => {
    const target = path.join(reconstructionDir, fileName);
    fs.writeFileSync(target, `${lines.join("\n").replace(/\s*$/, "")}\n`, "utf8");
    briefs.push(relativePosix(repoRoot, target));
  };

  if (normalization.domains.pulse.backup) {
    writeBrief("pulse-reconstruction-brief.md", [
      "# Pulse Runtime Reconstruction Brief",
      "",
      `Backup: ${normalization.domains.pulse.backup}`,
      "",
      "Known-safe runtime paths were copied into the v2 .pulse layout when possible.",
      "Review unmapped backup entries before treating them as current runtime truth.",
    ]);
  }

  if (normalization.domains.docs.backup) {
    writeBrief("docs-regeneration-brief.md", [
      "# Docs Regeneration Brief",
      "",
      `Backup: ${normalization.domains.docs.backup}`,
      "",
      "Read the backed-up docs and the current codebase, then regenerate target docs that conform to:",
      "",
      "- docs/ARCHITECTURE.md",
      "- docs/GLOSSARY.md",
      "- docs/decisions/",
      "- docs/product/",
      "",
      "Do not blindly copy the old docs structure back into active docs/.",
    ]);
  }

  if (normalization.domains.works.backup) {
    writeBrief("works-reconstruction-brief.md", [
      "# Works Reconstruction Brief",
      "",
      `Backup: ${normalization.domains.works.backup}`,
      "",
      "Read the backed-up work artifacts, infer the active work slices, and reconstruct them into:",
      "",
      "- works/epics/<E-id>-<slug>/README.md",
      "- works/epics/<E-id>-<slug>/<S-id>-<slug>/README.md",
      "- works/epics/<E-id>-<slug>/<S-id>-<slug>/SPEC.md",
      "- works/epics/<E-id>-<slug>/<S-id>-<slug>/tasks/<item-id>-<slug>/README.md",
      "- works/epics/<E-id>-<slug>/<S-id>-<slug>/tasks/<item-id>-<slug>/verification.md",
      "",
      "Synchronize reconstructed work with .pulse/workgraph/items.jsonl instead of preserving backed-up layout as active truth.",
    ]);
  }

  if (briefs.length > 0) {
    const manifestPath = path.join(reconstructionDir, "manifest.json");
    fs.writeFileSync(manifestPath, `${JSON.stringify({ schema_version: "1.0", generated_at: utcNow(), briefs }, null, 2)}\n`, "utf8");
  }

  return briefs;
}

function ensureDocsDomain(repoRoot, stamp) {
  const initial = classifyDocsDomain(repoRoot);
  const notes = [];
  const reconstructions = [];
  let backup = "";

  if (initial.status === "missing") {
    ensureDocsScaffold(repoRoot);
    return { ...initial, backup, notes, reconstructions };
  }

  if (initial.status === "non_compliant") {
    const activeEntries = listActiveDomainEntries(path.join(repoRoot, "docs"));
    if (activeEntries.length > 0) {
      const backupResult = backupDomainInPlace(repoRoot, "docs", stamp);
      backup = backupResult.backup;
      reconstructions.push(`docs active content -> ${backup}`);
      notes.push("docs domain was backed up and scaffolded; regenerate semantic docs from the onboarding reconstruction brief.");
    }
    ensureDocsScaffold(repoRoot);
  }

  return { ...initial, backup, notes, reconstructions };
}

function ensureWorksDomain(repoRoot, stamp) {
  const initial = classifyWorksDomain(repoRoot);
  const notes = [];
  const reconstructions = [];
  let backup = "";

  if (initial.status === "missing") {
    ensureWorksScaffold(repoRoot);
    return { ...initial, backup, notes, reconstructions };
  }

  if (initial.status === "non_compliant") {
    const activeEntries = listActiveDomainEntries(path.join(repoRoot, "works"));
    if (activeEntries.length > 0) {
      const backupResult = backupDomainInPlace(repoRoot, "works", stamp);
      backup = backupResult.backup;
      reconstructions.push(`works active content -> ${backup}`);
      notes.push("works domain was backed up and scaffolded; reconstruct work items from the onboarding reconstruction brief.");
    }
    ensureWorksScaffold(repoRoot);
  }

  return { ...initial, backup, notes, reconstructions };
}

export function buildDomainNormalization(repoRoot) {
  const stamp = backupStamp();
  const initial = classifyDomains(repoRoot);
  let pulseBackup = "";
  let pulseReconstructions = [];
  let pulseNotes = [];

  if (initial.pulse.status === "missing") {
    ensurePulseDomainLayout(repoRoot);
  } else if (initial.pulse.status === "non_compliant") {
    const backupResult = backupDomainInPlace(repoRoot, ".pulse", stamp);
    pulseBackup = backupResult.backup;
    ensurePulseDomainLayout(repoRoot);
    const reconstruction = restorePulseBackup(repoRoot, pulseBackup);
    pulseReconstructions = reconstruction.restored;
    pulseNotes = reconstruction.notes;
  } else {
    ensurePulseDomainLayout(repoRoot);
  }

  const docs = ensureDocsDomain(repoRoot, stamp);
  const works = ensureWorksDomain(repoRoot, stamp);
  const normalization = {
    backup_stamp: stamp,
    domains: {
      pulse: { ...initial.pulse, backup: pulseBackup, notes: pulseNotes, reconstructions: pulseReconstructions },
      docs,
      works,
    },
  };
  normalization.reconstruction_briefs = writeOnboardingReconstructionBriefs(repoRoot, normalization);
  return normalization;
}
