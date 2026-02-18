#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────
# audit.sh — Multi-LLM Smart Contract Audit Script
# 
# Collects all Rust sources from programs/ and sends them to a
# selected LLM for a security audit modeled after AUDIT_CONTEXT.md.
#
# Usage:
#   ./audit.sh --model opus   --api-key $ANTHROPIC_API_KEY
#   ./audit.sh --model codex  --api-key $OPENAI_API_KEY
#   ./audit.sh --model gemini --api-key $GOOGLE_API_KEY
#   ./audit.sh --model opus   --api-key $KEY --dry-run
# ─────────────────────────────────────────────────────────────────
set -euo pipefail

MODEL=""
API_KEY=""
DRY_RUN=false
OUTPUT_DIR="audit-reports"
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || echo ".")"

# ── Parse args ──────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --model)   MODEL="$2";   shift 2 ;;
    --api-key) API_KEY="$2"; shift 2 ;;
    --dry-run) DRY_RUN=true; shift   ;;
    --output)  OUTPUT_DIR="$2"; shift 2 ;;
    *) echo "Unknown arg: $1"; exit 1 ;;
  esac
done

if [[ -z "$MODEL" ]]; then
  echo "Error: --model is required (opus|codex|gemini)"
  exit 1
fi
if [[ -z "$API_KEY" && "$DRY_RUN" == false ]]; then
  echo "Error: --api-key is required (unless --dry-run)"
  exit 1
fi

# ── Collect source code ────────────────────────────────────────
echo "📂 Collecting Rust source files from programs/..."
SOURCE_CODE=""
while IFS= read -r file; do
  rel_path="${file#$REPO_ROOT/}"
  content=$(cat "$file")
  SOURCE_CODE+="
--- FILE: $rel_path ---
$content
--- END FILE ---
"
done < <(find "$REPO_ROOT/programs" -name "*.rs" -type f | sort)

CHAR_COUNT=${#SOURCE_CODE}
echo "📏 Collected $CHAR_COUNT characters of source code"

# ── Read reference audit format ─────────────────────────────────
AUDIT_REF=""
if [[ -f "$REPO_ROOT/AUDIT_CONTEXT.md" ]]; then
  echo "📋 Reading AUDIT_CONTEXT.md as format reference..."
  AUDIT_REF=$(cat "$REPO_ROOT/AUDIT_CONTEXT.md")
fi

# ── Build the audit prompt ──────────────────────────────────────
AUDIT_PROMPT="You are an expert Solana/Anchor smart contract security auditor. Perform a thorough security audit of the following Anchor program source code.

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
$AUDIT_REF

## Source Code to Audit
$SOURCE_CODE

Produce the full audit report now."

# ── Dry run ─────────────────────────────────────────────────────
if [[ "$DRY_RUN" == true ]]; then
  echo "🏃 DRY RUN — would send ${#AUDIT_PROMPT} chars to model: $MODEL"
  echo "   Source code: $CHAR_COUNT chars"
  echo "   Reference audit: ${#AUDIT_REF} chars"
  mkdir -p "$OUTPUT_DIR"
  echo "# Dry Run — $MODEL — $(date -u +%Y-%m-%d)" > "$OUTPUT_DIR/${MODEL}-dry-run.md"
  echo "Prompt size: ${#AUDIT_PROMPT} characters" >> "$OUTPUT_DIR/${MODEL}-dry-run.md"
  exit 0
fi

# ── Call the LLM API ────────────────────────────────────────────
DATE_STAMP=$(date -u +%Y-%m-%d)
mkdir -p "$OUTPUT_DIR"
OUTFILE="$OUTPUT_DIR/${MODEL}-${DATE_STAMP}.md"

echo "🤖 Sending audit request to model: $MODEL..."

case "$MODEL" in
  opus)
    # ── Anthropic Claude Opus 4.6 ──
    RESPONSE=$(curl -sS https://api.anthropic.com/v1/messages \
      -H "Content-Type: application/json" \
      -H "x-api-key: $API_KEY" \
      -H "anthropic-version: 2023-06-01" \
      --max-time 600 \
      -d "$(jq -n \
        --arg prompt "$AUDIT_PROMPT" \
        '{
          model: "claude-opus-4-6-20260210",
          max_tokens: 32000,
          messages: [{ role: "user", content: $prompt }]
        }')")

    # Extract text content from response
    echo "$RESPONSE" | jq -r '.content[0].text // .error.message // "Error: unexpected response format"' > "$OUTFILE"
    ;;

  codex)
    # ── OpenAI GPT-Codex-5.3 ──
    RESPONSE=$(curl -sS https://api.openai.com/v1/chat/completions \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer $API_KEY" \
      --max-time 600 \
      -d "$(jq -n \
        --arg prompt "$AUDIT_PROMPT" \
        '{
          model: "gpt-codex-5.3",
          max_tokens: 32000,
          messages: [{ role: "user", content: $prompt }]
        }')")

    echo "$RESPONSE" | jq -r '.choices[0].message.content // .error.message // "Error: unexpected response format"' > "$OUTFILE"
    ;;

  gemini)
    # ── Google Gemini 3 Pro ──
    RESPONSE=$(curl -sS "https://generativelanguage.googleapis.com/v1beta/models/gemini-3-pro:generateContent?key=$API_KEY" \
      -H "Content-Type: application/json" \
      --max-time 600 \
      -d "$(jq -n \
        --arg prompt "$AUDIT_PROMPT" \
        '{
          contents: [{ parts: [{ text: $prompt }] }],
          generationConfig: { maxOutputTokens: 32000 }
        }')")

    echo "$RESPONSE" | jq -r '.candidates[0].content.parts[0].text // .error.message // "Error: unexpected response format"' > "$OUTFILE"
    ;;

  *)
    echo "Error: unknown model '$MODEL'. Use: opus, codex, gemini"
    exit 1
    ;;
esac

# ── Validate output ─────────────────────────────────────────────
if [[ -s "$OUTFILE" ]]; then
  LINE_COUNT=$(wc -l < "$OUTFILE")
  echo "✅ Audit report written to $OUTFILE ($LINE_COUNT lines)"
else
  echo "❌ Error: audit report is empty"
  exit 1
fi
