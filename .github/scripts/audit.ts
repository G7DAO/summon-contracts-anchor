#!/usr/bin/env bun
// ─────────────────────────────────────────────────────────────────
// audit.ts — Multi-LLM Smart Contract Audit Script (ai-sdk)
//
// Collects all Rust sources from programs/ and sends them to a
// selected LLM for a security audit modeled after AUDIT_CONTEXT.md.
//
// Usage:
//   bun .github/scripts/audit.ts --model opus
//   bun .github/scripts/audit.ts --model codex
//   bun .github/scripts/audit.ts --model gemini
//   bun .github/scripts/audit.ts --model opus --dry-run
//
// Auth: reads ANTHROPIC_API_KEY, OPENAI_API_KEY, GOOGLE_API_KEY
//       from environment automatically (ai-sdk convention).
// ─────────────────────────────────────────────────────────────────
import { generateText } from "ai";
import { anthropic } from "@ai-sdk/anthropic";
import { openai } from "@ai-sdk/openai";
import { google } from "@ai-sdk/google";
import { readdir, readFile, mkdir, writeFile } from "node:fs/promises";
import { join, relative } from "node:path";
import { existsSync } from "node:fs";

// ── Model registry ─────────────────────────────────────────────
const MODELS: Record<string, () => ReturnType<typeof anthropic>> = {
  opus: () => anthropic("claude-opus-4-6-20260210"),
  codex: () => openai("gpt-codex-5.3") as any,
  gemini: () => google("gemini-3-pro") as any,
};

// ── Parse CLI args ─────────────────────────────────────────────
const args = process.argv.slice(2);
let model = "";
let dryRun = false;
let outputDir = "audit-reports";

for (let i = 0; i < args.length; i++) {
  switch (args[i]) {
    case "--model":
      model = args[++i];
      break;
    case "--dry-run":
      dryRun = true;
      break;
    case "--output":
      outputDir = args[++i];
      break;
  }
}

if (!model || !MODELS[model]) {
  console.error(
    `Error: --model is required (${Object.keys(MODELS).join("|")})`
  );
  process.exit(1);
}

// ── Collect Rust source files ──────────────────────────────────
async function collectRustFiles(dir: string): Promise<string[]> {
  const entries = await readdir(dir, { withFileTypes: true, recursive: true });
  return entries
    .filter((e) => !e.isDirectory() && e.name.endsWith(".rs"))
    .map((e) => join(e.parentPath, e.name))
    .sort();
}

const repoRoot = process.cwd();
const programsDir = join(repoRoot, "programs");
const rustFiles = await collectRustFiles(programsDir);

console.log(`📂 Found ${rustFiles.length} Rust source files in programs/`);

let sourceCode = "";
for (const file of rustFiles) {
  const relPath = relative(repoRoot, file);
  const content = await readFile(file, "utf-8");
  sourceCode += `\n--- FILE: ${relPath} ---\n${content}\n--- END FILE ---\n`;
}

console.log(`📏 Collected ${sourceCode.length} characters of source code`);

// ── Read reference audit format ────────────────────────────────
const auditRefPath = join(repoRoot, "AUDIT_CONTEXT.md");
const auditRef = existsSync(auditRefPath)
  ? await readFile(auditRefPath, "utf-8")
  : "";

if (auditRef) {
  console.log("📋 Loaded AUDIT_CONTEXT.md as format reference");
}

// ── Build audit prompt ─────────────────────────────────────────
const auditPrompt = `You are an expert Solana/Anchor smart contract security auditor. Perform a thorough security audit of the following Anchor program source code.

## Output Format
Generate your audit report in Markdown following this structure (modeled after the reference audit below):

### Phase 1 — Initial Orientation
- System Overview
- Actors & Access Control
- Entrypoints summary
- PDA Map

### Phase 2 — Granular Function Analysis
- For each instruction: purpose, inputs, block-by-block analysis, invariants, assumptions, risk observations

### Phase 3 — Global System Understanding
- State & invariant reconstruction
- End-to-end workflow reconstruction
- Trust boundary mapping
- Complexity & fragility clusters

### Summary — Top Findings
- Table of findings sorted by severity (CRITICAL > HIGH > MEDIUM > LOW > INFORMATIONAL)
- For each finding: area, description, recommendation

## Focus Areas
1. Access control bypasses
2. Missing validation / missing checks
3. Arithmetic overflow/underflow
4. PDA seed collisions
5. remaining_accounts parsing safety
6. Cross-instruction atomicity issues
7. Rent-exemption violations
8. Token-2022 / SPL token integration issues
9. Replay protection
10. Treasury fund safety / reservation invariants

## Reference Audit (for format reference only — do your own independent analysis)
${auditRef}

## Source Code to Audit
${sourceCode}

Produce the full audit report now.`;

// ── Dry run ────────────────────────────────────────────────────
await mkdir(outputDir, { recursive: true });

const dateStamp = new Date().toISOString().slice(0, 10);
const outFile = join(outputDir, `${model}-${dateStamp}.md`);

if (dryRun) {
  console.log(`🏃 DRY RUN — would send ${auditPrompt.length} chars to model: ${model}`);
  console.log(`   Source code: ${sourceCode.length} chars`);
  console.log(`   Reference audit: ${auditRef.length} chars`);
  await writeFile(outFile, `# Dry Run — ${model} — ${dateStamp}\nPrompt size: ${auditPrompt.length} characters\n`);
  process.exit(0);
}

// ── Call LLM via ai-sdk ────────────────────────────────────────
console.log(`🤖 Sending audit request to model: ${model}...`);

const { text } = await generateText({
  model: MODELS[model](),
  prompt: auditPrompt,
  maxOutputTokens: 32000,
});

await writeFile(outFile, text);

// ── Validate output ────────────────────────────────────────────
const lineCount = text.split("\n").length;
if (text.length > 0) {
  console.log(`✅ Audit report written to ${outFile} (${lineCount} lines)`);
} else {
  console.error("❌ Error: audit report is empty");
  process.exit(1);
}
