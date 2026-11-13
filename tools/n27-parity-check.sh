#!/bin/sh
# n27-parity-check.sh -- Sprint 27 T003 parity checker (CAP-004, J-03, INT-002, REC-004).
#
# Measures the released HyperFrames feature surface against framesmith and
# prints the parity verdict. It never asserts from documentation: every
# present row carries a mechanical probe, every released command is diffed
# live, and any unlisted difference fails naming the feature.
#
# What it checks, in order:
#   1. HF discovery: parse `npx --yes hyperframes@0.9.6 --help`, probe the
#      hidden commands (figma, media-use, play, usage, events), parse
#      `render --help` output formats and `docs --help` topics. Any
#      released command/format/topic with no inventory row is UNLISTED and
#      fails the verdict naming it; any inventory row that no longer
#      resolves is STALE and also fails.
#   2. Present-row probes: each present row's probe(s) must resolve in the
#      built binary (`--help` token, bare-usage token) or in the crate
#      sources (symbol grep, file existence). A removed mapping fails
#      naming the row.
#   3. Exception rows: every exception needs a justification (at least 30
#      characters) and an allowed category.
#   4. Verdict math: PASS only when present rows all probe green,
#      exceptions all carry justifications, and discovery diffs are clean.
#      The verdict names the release version (hyperframes@0.9.6) and
#      framesmith's version. Exit 0 = PASS (100% parity counting
#      exceptions); exit 1 = FAIL naming every unaccounted feature;
#      exit 2 = checker usage/inventory error; exit 3 = measurement
#      unavailable (npx unreachable), never a pass on a guess.
#
# Usage: n27-parity-check.sh [--inventory FILE] [--bin PATH] [--out FILE] [--selftest]
#   --selftest  after the clean run, re-run the checker against three
#      tampered inventory copies (a bogus probe token, a dropped row, a
#      dropped justification) and PASS only if each tamper fails naming
#      the affected feature (cli-init, add, cli-cloud).
#
# Durable rules this gate relies on: KDL style attributes are
# CSS-hyphenated (comp-box probes border-width); the glyph/layout crates
# are probed by symbol only; no render is issued, so single-pass bytes and
# the ffmpeg registry pin are untouched; the script runs on POSIX runners
# (Windows path joins and ffmpeg.exe handling are covered by the CI
# matrix, not by this shell).
#
# Inventory: framesmith/tools/n27-parity-inventory.json (115 rows).

set -u

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
DEFAULT_INV="$TOOLS_DIR/n27-parity-inventory.json"
DEFAULT_BIN="$FRAMESMITH_DIR/target/debug/framesmith"
HF_PKG="hyperframes@0.9.6"

INV="$DEFAULT_INV"
BIN_OVERRIDE=""
OUT=""
SELFTEST=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --inventory) INV="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    --out) OUT="${2:-}"; shift 2 ;;
    --selftest) SELFTEST=1; shift ;;
    -h|--help) sed -n '1,45p' "$0"; exit 0 ;;
    *) echo "n27-parity-check.sh: unknown flag $1" >&2; exit 2 ;;
  esac
done

if [ -z "$BIN_OVERRIDE" ]; then
  BIN="$DEFAULT_BIN"
else
  BIN="$BIN_OVERRIDE"
fi

if [ ! -x "$BIN" ]; then
  echo "building framesmith binary..." >&2
  (cd "$FRAMESMITH_DIR" && cargo build --bin framesmith) >&2 || exit 2
fi
if [ ! -f "$INV" ]; then
  echo "n27-parity-check.sh: inventory not found: $INV" >&2
  exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "n27-parity-check.sh: python3 is required" >&2
  exit 2
fi
if ! command -v npx >/dev/null 2>&1; then
  echo "n27-parity-check.sh: npx is required (measurement unavailable)" >&2
  exit 3
fi

export PARITY_INV="$INV"
export PARITY_BIN="$BIN"
export PARITY_HF_PKG="$HF_PKG"
export PARITY_OUT="$OUT"
export PARITY_SELFTEST="$SELFTEST"
export PARITY_FRAMESMITH_DIR="$FRAMESMITH_DIR"

python3 - <<'PYEOF'
import json, os, re, subprocess, sys, tempfile

INV = os.environ["PARITY_INV"]
BIN = os.environ["PARITY_BIN"]
HF_PKG = os.environ["PARITY_HF_PKG"]
OUT = os.environ["PARITY_OUT"]
SELFTEST = os.environ["PARITY_SELFTEST"] == "1"
FRAMESMITH_DIR = os.environ["PARITY_FRAMESMITH_DIR"]
HIDDEN = ["figma", "media-use", "play", "usage", "events"]
EXC_CATEGORIES = {"excluded", "browser-specific", "partial", "pending-sprint", "tool-identity", "no-verbatim-equivalent"}

def run(args, timeout=120):
    try:
        p = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
        return p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired:
        return -1, "", "timeout"

def hf_help(sub=None):
    args = ["npx", "--yes", HF_PKG]
    if sub:
        args.append(sub)
    args.append("--help")
    return run(args)

def parse_help_commands(text):
    cmds = []
    for line in text.splitlines():
        m = re.match(r"^  ([a-z][a-z0-9-]*)(?:  |\t)", line)
        if not m:
            continue
        cmds.append(m.group(1))
    return cmds

def parse_formats(text):
    m = re.search(r"Output format:\s*([a-z0-9,\s-]+?)\s*\(", text)
    if not m:
        return []
    return [v.strip() for v in m.group(1).split(",") if v.strip()]

def parse_topics(text):
    m = re.search(r"Topic:\s*([^\.]+\.)", text)
    if not m:
        return []
    body = m.group(1).rstrip(".")
    return [v.strip() for v in body.split(",") if v.strip()]

def discover_hf():
    """Live released-surface discovery. Returns (ctx, error)."""
    code, out, err = hf_help()
    if code != 0 or "hyperframes" not in out.lower():
        return None, f"npx --yes {HF_PKG} --help failed (exit {code}): {(out + err)[:240]}"
    listed = parse_help_commands(out)
    if not listed:
        return None, "parsed zero commands from HF --help"
    resolved_hidden = []
    hidden_detail = {}
    for cmd in HIDDEN:
        c2, o2, e2 = hf_help(cmd)
        body = o2 + e2
        ok = (c2 == 0 and "(hyperframes" in body and "Unknown command" not in body)
        hidden_detail[cmd] = "resolves" if ok else "does-not-resolve"
        if ok:
            resolved_hidden.append(cmd)
    code, out, err = hf_help("render")
    if code != 0:
        return None, "npx HF render --help failed"
    formats = parse_formats(out)
    if not formats:
        return None, "parsed zero output formats from HF render --help"
    code, out, err = hf_help("docs")
    if code != 0:
        return None, "npx HF docs --help failed"
    topics = parse_topics(out)
    if not topics:
        return None, "parsed zero docs topics from HF docs --help"
    ctx = {"listed": listed, "hidden": resolved_hidden, "hidden_detail": hidden_detail,
           "formats": formats, "topics": topics}
    return ctx, None

def load_inventory(path):
    with open(path) as f:
        return json.load(f)

# Framesmith-side captures (independent of the inventory, cached once).
FS = {}
FS["help"] = run([BIN, "--help"], timeout=60)[1]
FS["usage"] = run([BIN], timeout=60)[2]
ver_code, ver_out, _ = run([BIN, "--version"], timeout=60)
FS["version"] = (ver_code, ver_out)
FS["badcmd"] = run([BIN, "n27-no-such-cmd-probe"], timeout=60)
FS["check_missing"] = run([BIN, "check", "/nonexistent-n27-parity-probe"], timeout=60)

def probe_row(row):
    failures = []
    for p in row.get("probe", []):
        kind = p.get("kind")
        if kind == "help-has":
            if p["token"] not in FS["help"]:
                failures.append(f"help token {p['token']!r} missing from framesmith --help")
        elif kind == "usage-has":
            if p["token"] not in FS["usage"]:
                failures.append(f"usage token {p['token']!r} missing from framesmith bare usage")
        elif kind == "grep":
            path = os.path.join(FRAMESMITH_DIR, p["file"])
            try:
                with open(path) as f:
                    text = f.read()
            except OSError as e:
                failures.append(f"grep file {p['file']} unreadable: {e}")
                continue
            if p["needle"] not in text:
                failures.append(f"needle {p['needle']!r} missing from {p['file']}")
        elif kind == "exists":
            path = os.path.join(FRAMESMITH_DIR, p["file"])
            if not os.path.exists(path):
                failures.append(f"file {p['file']} does not exist")
        elif kind == "exit-version":
            c, o = FS["version"]
            if c != 0 or not o.strip().startswith("framesmith "):
                failures.append(f"--version exit={c} output={o.strip()[:80]!r}")
        elif kind == "exit-badcmd":
            c, _o, e = FS["badcmd"]
            if c != 2 or "unknown subcommand" not in e:
                failures.append(f"badcmd exit={c} stderr={e.strip()[:120]!r}")
        elif kind == "exit-check-missing":
            c, _o, _e = FS["check_missing"]
            if c != 1:
                failures.append(f"check-missing exit={c}, expected 1")
        else:
            failures.append(f"unknown probe kind {kind!r}")
    return failures

def run_checks(inv, ctx):
    failures = []
    rows = inv["rows"]
    seen = set()
    for r in rows:
        rid = r["id"]
        if rid in seen:
            failures.append((rid, "duplicate row id"))
        seen.add(rid)
        status = r.get("status")
        if status == "present":
            for f in probe_row(r):
                failures.append((rid, f))
        elif status == "exception":
            j = r.get("justification") or ""
            cat = r.get("category")
            if cat not in EXC_CATEGORIES:
                failures.append((rid, f"exception category {cat!r} not allowed"))
            elif len(j.strip()) < 30:
                failures.append((rid, "exception justification missing or under 30 chars"))
        else:
            failures.append((rid, f"status {status!r} is neither present nor exception"))
    if ctx is not None:
        inv_cmds = {r["hf_cli"]: r["id"] for r in rows if r.get("hf_cli")}
        discovered = set(ctx["listed"]) | set(ctx["hidden"])
        for cmd in sorted(discovered - set(inv_cmds)):
            failures.append((f"hf-cmd:{cmd}", f"UNLISTED released command {cmd!r} has no inventory row"))
        for cmd in sorted(set(inv_cmds) - discovered):
            failures.append((inv_cmds[cmd], f"STALE row names {cmd!r} which does not resolve in released {HF_PKG}"))
        inv_formats = {r["hf_format"]: r["id"] for r in rows if r.get("hf_format")}
        for fmt in sorted(set(ctx["formats"]) - set(inv_formats)):
            failures.append((f"hf-format:{fmt}", f"UNLISTED released render format {fmt!r} has no inventory row"))
        for fmt in sorted(set(inv_formats) - set(ctx["formats"])):
            failures.append((inv_formats[fmt], f"STALE row names format {fmt!r} not in released {HF_PKG}"))
        topics_rows = [r for r in rows if r.get("hf_topics") is not None]
        if len(topics_rows) != 1:
            failures.append(("docs-topics", f"expected exactly one docs-topics row, found {len(topics_rows)}"))
        else:
            inv_topics = set(topics_rows[0]["hf_topics"])
            for t in sorted(set(ctx["topics"]) - inv_topics):
                failures.append(("docs-topics", f"UNLISTED released docs topic {t!r}"))
            for t in sorted(inv_topics - set(ctx["topics"])):
                failures.append(("docs-topics", f"STALE docs topic {t!r} not in released {HF_PKG}"))
    return failures

def main():
    try:
        inv = load_inventory(INV)
    except (OSError, json.JSONDecodeError) as e:
        print(f"parity-check: cannot load inventory {INV}: {e}", file=sys.stderr)
        return 2
    ctx, err = discover_hf()
    if err:
        print(f"MEASUREMENT UNAVAILABLE: {err}", file=sys.stderr)
        print(f"VERDICT: FAIL (unmeasured) vs {HF_PKG}", file=sys.stderr)
        return 3
    print(f"released surface (live): {len(ctx['listed'])} listed commands, "
          f"hidden {ctx['hidden']}, formats {ctx['formats']}, topics {ctx['topics']}")
    failures = run_checks(inv, ctx)
    rows = inv["rows"]
    n_present = sum(1 for r in rows if r["status"] == "present")
    n_exc = sum(1 for r in rows if r["status"] == "exception")
    if failures:
        for rid, why in failures:
            print(f"FAIL: {rid}: {why}")
        print(f"VERDICT: FAIL vs {HF_PKG} -- {len(failures)} unaccounted "
              f"({n_present} present + {n_exc} exceptions of {len(rows)} rows)")
        return 1
    ver = FS["version"][1].strip()
    print(f"VERDICT: 100% parity counting exceptions vs {HF_PKG} "
          f"({n_present} present + {n_exc} exceptions of {len(rows)} rows; framesmith {ver[11:] if ver.startswith('framesmith ') else ver})")
    for r in rows:
        if r["status"] == "exception":
            print(f"EXCEPTION: {r['id']} [{r['category']}] {r['justification']}")
    if OUT:
        write_report(inv, ctx, ver)
    if SELFTEST:
        ok = selftest(inv, ctx)
        if not ok:
            return 1
    return 0

def tamper_check(copy_inv, ctx, expect_names, label):
    fails = run_checks(copy_inv, ctx)
    blob = "\n".join(f"{rid} {why}" for rid, why in fails)
    missing = [n for n in expect_names if n not in blob and not any(n in rid for rid, _ in fails)]
    if not fails:
        print(f"SELFTEST FAIL: tamper {label} escaped (no failures)")
        return False
    if missing:
        print(f"SELFTEST FAIL: tamper {label} did not name {missing}: {fails}")
        return False
    print(f"SELFTEST PASS: tamper {label} failed naming {expect_names}")
    return True

def selftest(inv, ctx):
    tmp = tempfile.mkdtemp(prefix="n27-parity-selftest-")
    ok = True
    # A: bogus probe token on a present row -> must fail naming cli-init
    a = json.loads(json.dumps(inv))
    for r in a["rows"]:
        if r["id"] == "cli-init":
            r["probe"] = [{"kind": "help-has", "token": "ZZZ-NO-SUCH-TOKEN-N27"}]
    pa = os.path.join(tmp, "inv-a.json")
    with open(pa, "w") as f:
        json.dump(a, f)
    ok = tamper_check(load_inventory(pa), ctx, ["cli-init"], "bogus-probe") and ok
    # B: dropped row -> discovery must fail naming add
    b = json.loads(json.dumps(inv))
    b["rows"] = [r for r in b["rows"] if r["id"] != "cli-add"]
    pb = os.path.join(tmp, "inv-b.json")
    with open(pb, "w") as f:
        json.dump(b, f)
    ok = tamper_check(load_inventory(pb), ctx, ["add"], "dropped-row") and ok
    # C: dropped justification -> must fail naming cli-cloud
    c = json.loads(json.dumps(inv))
    for r in c["rows"]:
        if r["id"] == "cli-cloud":
            r["justification"] = ""
    pc = os.path.join(tmp, "inv-c.json")
    with open(pc, "w") as f:
        json.dump(c, f)
    ok = tamper_check(load_inventory(pc), ctx, ["cli-cloud"], "dropped-justification") and ok
    return ok

def write_report(inv, ctx, ver):
    import datetime
    rows = inv["rows"]
    n_present = sum(1 for r in rows if r["status"] == "present")
    n_exc = sum(1 for r in rows if r["status"] == "exception")
    lines = []
    lines.append(f"# Parity verdict - framesmith vs {HF_PKG}")
    lines.append("")
    lines.append(f"- date (UTC): {datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')}")
    lines.append(f"- framesmith: {ver}")
    lines.append(f"- released surface (live): {len(ctx['listed'])} listed commands, hidden {ctx['hidden']}, "
                 f"formats {ctx['formats']}, docs topics {ctx['topics']}")
    lines.append(f"- rows: {len(rows)} ({n_present} present, {n_exc} exceptions)")
    lines.append(f"- VERDICT: 100% parity counting exceptions vs {HF_PKG}")
    lines.append("")
    lines.append("Generated by `framesmith/tools/n27-parity-check.sh`. Re-run it to refresh; "
                 "the checker itself measures (diffs) and fails on any unlisted difference.")
    lines.append("")
    lines.append("## Exceptions (count toward the 100%)")
    lines.append("")
    lines.append("| row | feature | category | justification |")
    lines.append("|---|---|---|---|")
    for r in rows:
        if r["status"] == "exception":
            lines.append(f"| {r['id']} | {r['hf']} | {r['category']} | {r['justification']} |")
    lines.append("")
    lines.append("## Present mappings (probe summary)")
    lines.append("")
    lines.append("| row | released feature | framesmith |")
    lines.append("|---|---|---|")
    for r in rows:
        if r["status"] == "present":
            fm = r.get("framesmith_form") or ""
            lines.append(f"| {r['id']} | {r['hf']} | {fm} |")
    lines.append("")
    with open(OUT, "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"report written: {OUT}")

if __name__ == "__main__":
    sys.exit(main())
PYEOF
