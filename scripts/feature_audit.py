#!/usr/bin/env python3

import json
import os
import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ENTRYPOINT = ROOT / "entrypoints" / "cli.tsx"
FEATURE_ENV = ROOT / ".mossensrc" / "feature-flags.env"
FEATURED_RUNNER = ROOT / "run-bun-featured.sh"
MAIN_TSX = ROOT / "main.tsx"
COMMANDS_TS = ROOT / "commands.ts"

MISSING_MODULE_RE = re.compile(
    r"Cannot find module '([^']+)' from '([^']+)'"
)
MISSING_EXPORT_RE = re.compile(
    r"does not provide an export named '([^']+)'"
)

FEATURE_PROFILES = [
    {
        "id": "transcript-classifier",
        "features": ["TRANSCRIPT_CLASSIFIER"],
        "expected_commands": ["auto-mode"],
    },
    {
        "id": "kairos-core",
        "features": ["KAIROS"],
        "expected_commands": ["assistant"],
        "notes": [
            "`task` remains internal-only in main.tsx and is intentionally absent in external/custom builds."
        ],
    },
    {
        "id": "voice-mode",
        "features": ["VOICE_MODE"],
        "expected_commands": [],
        "runtime_expectations": {
            "platformCore.featureGates.voiceMode": True,
        },
    },
    {
        "id": "chicago-mcp",
        "features": ["CHICAGO_MCP"],
        "expected_commands": [],
    },
    {
        "id": "daemon-only",
        "features": ["DAEMON"],
        "expected_commands": [],
    },
]


def read_default_features() -> list[str]:
    env_override = os.environ.get("MOSSENSRC_BUN_FEATURES")
    raw = env_override
    if raw is None and FEATURE_ENV.exists():
      for line in FEATURE_ENV.read_text().splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
          continue
        match = re.match(r"export\s+MOSSENSRC_BUN_FEATURES=\"\$\{MOSSENSRC_BUN_FEATURES:-([^}]*)\}\"", stripped)
        if match:
          raw = match.group(1)
          break
    if raw is None:
      return []
    parts = re.split(r"[,\s]+", raw)
    return [part for part in parts if part]


def parse_commands(output: str) -> list[str]:
    commands: list[str] = []
    capture = False
    for raw_line in output.splitlines():
        line = raw_line.rstrip()
        if line == "Commands:":
            capture = True
            continue
        if not capture:
            continue
        if not line.strip():
            continue
        token = line.strip().split()[0]
        if token == "help":
            continue
        commands.append(token)
    return commands


def classify_failure(output: str) -> dict[str, object]:
    missing_module = MISSING_MODULE_RE.search(output)
    if missing_module:
        return {
            "type": "missing-module",
            "specifier": missing_module.group(1),
            "importer": missing_module.group(2),
        }
    missing_export = MISSING_EXPORT_RE.search(output)
    if missing_export:
        return {
            "type": "missing-export",
            "name": missing_export.group(1),
        }
    return {
        "type": "unknown",
        "summary": output.strip().splitlines()[:6],
    }


def collect_daemon_snapshot_signals() -> dict[str, object]:
    pattern = re.compile(r"feature\((?:'|\\\")DAEMON(?:'|\\\")\)")
    excluded = {
        ROOT / "platform" / "featureGatesRuntime.ts",
        ROOT / "scripts" / "feature_audit.py",
    }
    daemon_feature_refs = 0
    for path in ROOT.rglob("*"):
        if path in excluded:
            continue
        if path.suffix not in {".ts", ".tsx", ".js", ".mjs", ".cjs"}:
            continue
        if "node_modules" in path.parts:
            continue
        try:
            text = path.read_text(errors="ignore")
        except OSError:
            continue
        daemon_feature_refs += len(pattern.findall(text))

    main_text = MAIN_TSX.read_text(errors="ignore")
    commands_text = COMMANDS_TS.read_text(errors="ignore")
    return {
        "daemonFeatureRefs": daemon_feature_refs,
        "assistantFlagPresent": "--assistant" in main_text,
        "remoteControlWorkerRegistered": "remote-control-server" in main_text
        or "rc-server" in main_text,
        "workerCommandCompiled": "remoteControlServerCommand" in commands_text
        and "./commands/remoteControlServer/index.js" in commands_text,
    }


def get_nested(data: dict[str, object], dotted_key: str) -> object:
    current: object = data
    for part in dotted_key.split("."):
        if not isinstance(current, dict) or part not in current:
            return None
        current = current[part]
    return current


def run_runtime_expectations(
    features: list[str], expectations: dict[str, object]
) -> dict[str, object]:
    env = os.environ.copy()
    env["MOSSENSRC_BUN_FEATURES"] = ",".join(features)
    proc = subprocess.run(
        [str(FEATURED_RUNNER), str(ENTRYPOINT), "auth", "status"],
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=90,
        env=env,
    )
    output = ((proc.stdout or "") + (proc.stderr or "")).strip()
    if proc.returncode != 0:
        return {
            "ok": False,
            "returncode": proc.returncode,
            "blocker": classify_failure(output),
        }

    payload = json.loads(output)
    checks: list[dict[str, object]] = []
    ok = True
    for dotted_key, expected in expectations.items():
        actual = get_nested(payload, dotted_key)
        passed = actual == expected
        checks.append(
            {
                "key": dotted_key,
                "expected": expected,
                "actual": actual,
                "ok": passed,
            }
        )
        if not passed:
            ok = False

    return {"ok": ok, "checks": checks}


def run_profile(profile: dict[str, object]) -> dict[str, object]:
    features = profile["features"]
    command = ["bun", *[f"--feature={name}" for name in features], str(ENTRYPOINT), "--help"]
    proc = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        capture_output=True,
        timeout=60,
    )
    output = ((proc.stdout or "") + (proc.stderr or "")).strip()
    commands = parse_commands(output) if proc.returncode == 0 else []
    expected = profile["expected_commands"]
    missing_expected = [
        name for name in expected if not any(token.split("|")[0] == name for token in commands)
    ]

    runtime_expectations = profile.get("runtime_expectations")
    runtime_result = None
    if runtime_expectations:
        runtime_result = run_runtime_expectations(features, runtime_expectations)

    runtime_ok = runtime_result is None or bool(runtime_result.get("ok"))
    snapshot_missing = bool(
        runtime_result
        and any(
            check.get("key")
            in {
                "platformCore.directConnectSnapshotMissing",
                "platformCore.sshSnapshotMissing",
            }
            and check.get("actual") is True
            for check in runtime_result.get("checks", [])
            if isinstance(check, dict)
        )
    )

    runtime_blocking = bool(profile.get("runtime_blocking"))

    if proc.returncode == 0 and not missing_expected and runtime_ok and snapshot_missing:
        status = "snapshot-missing"
    elif proc.returncode == 0 and not missing_expected and runtime_ok:
        status = "ok"
    elif proc.returncode == 0 and not missing_expected and not runtime_ok and runtime_blocking:
        status = "blocked"
    elif proc.returncode != 0:
        status = "blocked"
    else:
        status = "partial"

    result = {
        "id": profile["id"],
        "features": features,
        "status": status,
        "returncode": proc.returncode,
        "expectedCommands": expected,
        "exposedCommands": commands,
        "missingExpectedCommands": missing_expected,
    }
    notes = profile.get("notes")
    if notes:
        result["notes"] = notes
    if runtime_result is not None:
        result["runtime"] = runtime_result
    if profile["id"] == "daemon-bridge":
        source_signals = collect_daemon_snapshot_signals()
        result["sourceSignals"] = source_signals
        if (
            source_signals["daemonFeatureRefs"] == 0
            and not source_signals["remoteControlWorkerRegistered"]
            and not source_signals["workerCommandCompiled"]
        ):
            result["status"] = "dormant"
            daemon_notes = result.setdefault("notes", [])
            daemon_notes.append(
                "`DAEMON` has no live feature-gated source branches in this snapshot beyond diagnostics, and no `remote-control-server` worker command is registered in main.tsx."
            )
        elif source_signals["workerCommandCompiled"]:
            result["status"] = "worker-only"
            daemon_notes = result.setdefault("notes", [])
            daemon_notes.append(
                "`DAEMON` currently compiles a hidden `remoteControlServer` worker path via commands.ts, but it does not expose a separate user-facing command surface in this snapshot."
            )
    if proc.returncode != 0:
        result["blocker"] = classify_failure(output)
    return result


def main() -> int:
    report = {
        "defaultFeatures": read_default_features(),
        "profiles": [run_profile(profile) for profile in FEATURE_PROFILES],
    }
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
