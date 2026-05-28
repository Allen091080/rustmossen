#!/usr/bin/env python3
"""W65 — current Rust GitHub skill install contract smoke."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from harness_rust_context import Step, cargo_test, run_context_harness, source_check


def main() -> int:
    return run_context_harness(
        test_id="W65",
        script_name=Path(__file__).name,
        steps=[
            Step(
                name="github_skill_install_plan_execute_one_shot",
                command=cargo_test(
                    "-p",
                    "mossen-utils",
                    "--lib",
                    "skills_utils::tests::github_skill_install_plan_execute_is_bounded_one_shot_and_notifies",
                ),
            ),
            Step(
                name="github_skill_install_requires_skill_md",
                command=cargo_test(
                    "-p",
                    "mossen-utils",
                    "--lib",
                    "skills_utils::tests::github_skill_install_plan_requires_skill_md",
                ),
            ),
            Step(
                name="slash_skills_install_remove_commands",
                command=cargo_test(
                    "-p",
                    "mossen-commands",
                    "crafts::tests::",
                ),
            ),
        ],
        checks=[
            source_check(
                "github_skill_install_limits_and_token_store",
                "crates/mossen-utils/src/skills_utils/mod.rs",
                [
                    "pub const GITHUB_SKILL_INSTALL_TOKEN_TTL_MS: u64 = 10 * 60 * 1000;",
                    "const MAX_FILES: usize = 100;",
                    "const MAX_TOTAL_BYTES: usize = 2 * 1024 * 1024;",
                    "INSTALL_PLANS.lock().unwrap().insert(token.clone(), stored);",
                    "store.remove(&plan.token)",
                ],
            ),
            source_check(
                "github_skill_install_target_and_validation",
                "crates/mossen-utils/src/skills_utils/mod.rs",
                [
                    "pub fn parse_github_skill_target",
                    "host != \"github.com\"",
                    "file_name()",
                    "name == \"SKILL.md\"",
                    "safe_join(&plan.install_dir, &file.path)",
                ],
            ),
            source_check(
                "github_skill_install_refreshes_skill_change_detector",
                "crates/mossen-utils/src/skills_utils/mod.rs",
                [
                    "SKILL_CHANGE_DETECTOR.notify_change(install_dir);",
                    "pub fn notify_change(&self",
                    "github_skill_install_plan_execute_is_bounded_one_shot_and_notifies",
                ],
            ),
            source_check(
                "slash_skills_install_remove_are_real_commands",
                "crates/mossen-commands/src/crafts.rs",
                [
                    "install_github_skill_from_target(",
                    "fetch_github_skill_tree,",
                    "mossen_utils::skills_utils::execute_github_skill_install_plan(&plan).await?",
                    "mossen_skills::add_skill_directories(&[root.to_path_buf()]).await",
                    "remove_project_skill(&name, &ctx.cwd).await",
                    "tokio::fs::remove_dir_all(&skill_dir).await?",
                ],
            ),
        ],
        design_note=(
            "W65 validates the Rust GitHub skill install utility: dry-run plan, "
            "bounded files/bytes, SKILL.md requirement, one-shot confirm token, "
            "safe install path, skill-change notification, and the user-visible "
            "/skills install/remove command wiring."
        ),
    )


if __name__ == "__main__":
    raise SystemExit(main())
