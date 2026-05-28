#!/usr/bin/env python3
"""R11.2 - isolated package/install smoke for the Mossen CLI binary."""

from __future__ import annotations

import json
import os
import shutil
import stat
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_fixture import make_fixture, write_assertions


def preserve_rust_toolchain_env(env: dict[str, str]) -> None:
    real_home = Path(os.environ.get("HOME", str(Path.home())))
    cargo_home = Path(os.environ.get("CARGO_HOME", str(real_home / ".cargo")))
    rustup_home = Path(os.environ.get("RUSTUP_HOME", str(real_home / ".rustup")))
    if cargo_home.exists():
        env["CARGO_HOME"] = str(cargo_home)
    if rustup_home.exists():
        env["RUSTUP_HOME"] = str(rustup_home)


def run(
    command: list[str],
    *,
    env: dict[str, str],
    cwd: Path,
    timeout: int,
) -> tuple[subprocess.CompletedProcess[str], float]:
    start = time.perf_counter()
    result = subprocess.run(
        command,
        cwd=str(cwd),
        env=env,
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    return result, time.perf_counter() - start


def file_mode(path: Path) -> int | None:
    if not path.exists():
        return None
    return stat.S_IMODE(path.stat().st_mode)


def assertion(name: str, ok: bool, **detail: Any) -> dict[str, Any]:
    return {"name": name, "ok": ok, **detail}


SENSITIVE_PARTS = ("KEY", "TOKEN", "SECRET", "PASSWORD", "AUTH", "CREDENTIAL")


def redact_value(key: str, value: str) -> str:
    upper = key.upper()
    if any(part in upper for part in SENSITIVE_PARTS):
        if not value:
            return ""
        if len(value) <= 8:
            return "<redacted>"
        return f"{value[:4]}...{value[-4:]}<redacted>"
    return value


def sanitized_env_lines(env: dict[str, str]) -> list[str]:
    prefixes = ("HOME", "PATH", "MOSSEN_", "XDG_", "CARGO_HOME", "RUSTUP_HOME")
    return [
        f"{key}={redact_value(key, value)}"
        for key, value in sorted(env.items())
        if key.startswith(prefixes)
    ]


def json_load(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}


def parse_json_stdout(result: subprocess.CompletedProcess[str]) -> dict[str, Any]:
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return {}


def main() -> int:
    ctx = make_fixture("R11.2_package_install_smoke")
    install_root = ctx.root_dir / "install-root"
    external_cwd = ctx.root_dir / "external-cwd"
    install_root.mkdir(parents=True, exist_ok=True)
    external_cwd.mkdir(parents=True, exist_ok=True)
    env = dict(ctx.env)
    preserve_rust_toolchain_env(env)
    env["MOSSEN_CONFIG_DIR"] = str(ctx.mossen_config_home)
    env["MOSSEN_START_BUILD"] = "never"

    install_command = [
        "cargo",
        "install",
        "--path",
        str(ROOT / "crates" / "mossen-cli"),
        "--root",
        str(install_root),
        "--locked",
        "--force",
        "--bin",
        "mossen",
    ]
    install, install_elapsed = run(install_command, env=env, cwd=ROOT, timeout=900)
    installed_bin = install_root / "bin" / "mossen"
    bin_dir = install_root / "bin"
    runtime_env = dict(env)
    runtime_env["PATH"] = os.pathsep.join([str(bin_dir), env.get("PATH", "")])
    runtime_env.update(
        {
            "MOSSEN_CODE_USE_CUSTOM_BACKEND": "1",
            "MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL": "openai-compatible",
            "MOSSEN_CODE_CUSTOM_NAME": "release-smoke-fallback",
            "MOSSEN_CODE_CUSTOM_BASE_URL": "https://provider.invalid/v1",
            "MOSSEN_CODE_CUSTOM_API_KEY": "sk-r11-package-smoke",
            "MOSSEN_CODE_CUSTOM_MODEL": "release-smoke-model",
        }
    )

    version_command = ["mossen", "--version"]
    profile_list_command = ["mossen", "--list-model-profiles"]
    migrate_command = [
        "mossen",
        "--migrate-fallback-profile",
        "--name",
        "release-smoke",
        "--activate",
        "always",
        "--force",
    ]
    if installed_bin.exists():
        version, version_elapsed = run(
            version_command, env=runtime_env, cwd=external_cwd, timeout=30
        )
        first_launch, first_launch_elapsed = run(
            profile_list_command, env=runtime_env, cwd=external_cwd, timeout=30
        )
        migrate, migrate_elapsed = run(
            migrate_command, env=runtime_env, cwd=external_cwd, timeout=30
        )
        post_migrate, post_migrate_elapsed = run(
            profile_list_command, env=runtime_env, cwd=external_cwd, timeout=30
        )
    else:
        version = subprocess.CompletedProcess(version_command, 127, "", "installed binary missing")
        first_launch = subprocess.CompletedProcess(
            profile_list_command, 127, "", "installed binary missing"
        )
        migrate = subprocess.CompletedProcess(migrate_command, 127, "", "installed binary missing")
        post_migrate = subprocess.CompletedProcess(
            profile_list_command, 127, "", "installed binary missing"
        )
        version_elapsed = first_launch_elapsed = migrate_elapsed = post_migrate_elapsed = 0.0

    profile_payload = parse_json_stdout(first_launch)
    migrate_payload = parse_json_stdout(migrate)
    post_migrate_payload = parse_json_stdout(post_migrate)
    settings_path = ctx.mossen_config_home / "settings.json"
    settings = json_load(settings_path)
    migrated_profile = (settings.get("mossen.profiles") or {}).get("release-smoke") or {}
    resolved_mossen = shutil.which("mossen", path=runtime_env["PATH"])

    (ctx.artifacts_dir / "command.txt").write_text(
        "\n".join(
            [
                " ".join(install_command),
                " ".join(version_command),
                " ".join(profile_list_command),
                " ".join(migrate_command),
            ]
        ),
        encoding="utf-8",
    )
    (ctx.artifacts_dir / "env.txt").write_text(
        "\n".join(sanitized_env_lines(runtime_env)),
        encoding="utf-8",
    )
    (ctx.artifacts_dir / "stdout.txt").write_text(
        "=== cargo install stdout ===\n"
        + install.stdout
        + "\n=== mossen --version stdout ===\n"
        + version.stdout
        + "\n=== first launch --list-model-profiles stdout ===\n"
        + first_launch.stdout
        + "\n=== migrate fallback stdout ===\n"
        + migrate.stdout
        + "\n=== post-migrate --list-model-profiles stdout ===\n"
        + post_migrate.stdout,
        encoding="utf-8",
    )
    (ctx.artifacts_dir / "stderr.txt").write_text(
        "=== cargo install stderr ===\n"
        + install.stderr
        + "\n=== mossen --version stderr ===\n"
        + version.stderr
        + "\n=== first launch --list-model-profiles stderr ===\n"
        + first_launch.stderr
        + "\n=== migrate fallback stderr ===\n"
        + migrate.stderr
        + "\n=== post-migrate --list-model-profiles stderr ===\n"
        + post_migrate.stderr,
        encoding="utf-8",
    )
    (ctx.artifacts_dir / "exit_code.txt").write_text(
        json.dumps(
            {
                "install": install.returncode,
                "version": version.returncode,
                "first_launch": first_launch.returncode,
                "migrate": migrate.returncode,
                "post_migrate": post_migrate.returncode,
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    (ctx.artifacts_dir / "latency.json").write_text(
        json.dumps(
            {
                "install_elapsed_secs": install_elapsed,
                "version_elapsed_secs": version_elapsed,
                "first_launch_elapsed_secs": first_launch_elapsed,
                "migrate_elapsed_secs": migrate_elapsed,
                "post_migrate_elapsed_secs": post_migrate_elapsed,
            },
            indent=2,
        ),
        encoding="utf-8",
    )

    mode = file_mode(installed_bin)
    assertions = [
        assertion(
            "cargo_install_release_exits_zero",
            install.returncode == 0,
            exit_code=install.returncode,
            elapsed_secs=round(install_elapsed, 3),
            debug_build_flag_used="--debug" in install_command,
        ),
        assertion(
            "installed_binary_exists_and_is_executable",
            installed_bin.exists() and os.access(installed_bin, os.X_OK),
            installed_binary=str(installed_bin),
            mode_octal=f"0o{oct(mode)[2:]}" if mode is not None else None,
        ),
        assertion(
            "installed_binary_is_resolved_from_path",
            resolved_mossen == str(installed_bin),
            resolved_mossen=resolved_mossen,
            expected=str(installed_bin),
        ),
        assertion(
            "installed_binary_runs_outside_repo_cwd",
            version.returncode == 0 and str(ROOT) not in version.stdout + version.stderr,
            exit_code=version.returncode,
            cwd=str(external_cwd),
            stdout=version.stdout.strip(),
            stderr=version.stderr.strip(),
            elapsed_secs=round(version_elapsed, 3),
        ),
        assertion(
            "installed_binary_version_reports_mossen",
            "mossen" in version.stdout.lower(),
            stdout=version.stdout.strip(),
        ),
        assertion(
            "first_launch_profile_query_exits_zero",
            first_launch.returncode == 0 and isinstance(profile_payload.get("allProfiles"), list),
            exit_code=first_launch.returncode,
            profile_count=profile_payload.get("count"),
            all_profile_count=profile_payload.get("countAll"),
            elapsed_secs=round(first_launch_elapsed, 3),
        ),
        assertion(
            "startup_latency_with_installed_binary_is_bounded",
            first_launch.returncode == 0 and first_launch_elapsed < 5.0,
            exit_code=first_launch.returncode,
            elapsed_secs=round(first_launch_elapsed, 3),
            threshold_secs=5.0,
        ),
        assertion(
            "fallback_profile_migration_exits_zero",
            migrate.returncode == 0
            and migrate_payload.get("status") == "Migrated"
            and migrate_payload.get("profile_name") == "release-smoke",
            exit_code=migrate.returncode,
            status=migrate_payload.get("status"),
            migrated_name=migrate_payload.get("profile_name"),
            elapsed_secs=round(migrate_elapsed, 3),
        ),
        assertion(
            "config_migration_writes_isolated_settings",
            settings_path.exists()
            and settings.get("mossen.activeProfile") == "release-smoke"
            and migrated_profile.get("model") == "release-smoke-model"
            and migrated_profile.get("baseURL") == "https://provider.invalid/v1"
            and migrated_profile.get("provider") == "openai-compatible",
            settings_path=str(settings_path),
            active_profile=settings.get("mossen.activeProfile"),
            migrated_model=migrated_profile.get("model"),
            migrated_base_url=migrated_profile.get("baseURL"),
            migrated_provider=migrated_profile.get("provider"),
        ),
        assertion(
            "post_migration_profile_visible_without_secrets",
            post_migrate.returncode == 0
            and post_migrate_payload.get("activeProfile") == "release-smoke"
            and "sk-r11-package-smoke" not in post_migrate.stdout,
            exit_code=post_migrate.returncode,
            active_profile=post_migrate_payload.get("activeProfile"),
            elapsed_secs=round(post_migrate_elapsed, 3),
        ),
        assertion(
            "artifact_env_redacts_sensitive_values",
            "sk-r11-package-smoke"
            not in (ctx.artifacts_dir / "env.txt").read_text(encoding="utf-8"),
        ),
    ]
    all_ok = all(item["ok"] for item in assertions)
    write_assertions(
        ctx,
        status="passed" if all_ok else "failed",
        assertions=assertions,
        extra_artifacts={
            "install_root": str(install_root),
            "installed_binary": str(installed_bin),
            "external_cwd": str(external_cwd),
            "settings_path": str(settings_path),
        },
    )
    print(
        json.dumps(
            {
                "ok": all_ok,
                "install_root": str(install_root),
                "installed_binary": str(installed_bin),
                "version": version.stdout.strip(),
                "first_launch_elapsed_secs": round(first_launch_elapsed, 3),
                "migrated_profile": settings.get("mossen.activeProfile"),
                "artifacts": str(ctx.artifacts_dir),
            },
            indent=2,
            ensure_ascii=False,
        )
    )
    return 0 if all_ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
