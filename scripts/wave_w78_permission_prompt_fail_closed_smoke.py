#!/usr/bin/env python3
"""W78 - SDK permission prompt fail-closed smoke.

Guards the headless/SDK permission path so an unavailable permission prompt
cannot silently allow tool execution.
"""

from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
PRINT_HANDLERS = ROOT / "crates/mossen-cli/src/print_handlers.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def fail(failures: list[str], message: str) -> None:
    failures.append(message)


def main() -> int:
    failures: list[str] = []
    print_handlers = PRINT_HANDLERS.read_text()
    run_all = RUN_ALL.read_text()

    if "permission prompt invoked (placeholder allow)" in print_handlers:
        fail(failures, "print_handlers.rs must not keep the placeholder allow path")
    for snippet in [
        "pub type PermissionPromptCallback",
        "fn value_declares_tool(",
        "fn find_permission_prompt_server(",
        "pub fn create_can_use_tool_with_permission_prompt_callback(",
        "prompt_callback: Option<PermissionPromptCallback>",
        "let Some(server_name) = find_permission_prompt_server",
        "let Some(callback) = callback else",
        "match callback(prompt_tool.clone(), prompt_input).await",
        "Ok(PermissionPromptDecision::Allow { updated_input })",
        "Ok(PermissionPromptDecision::Deny { message })",
        "Err(error)",
        "not advertised by any SDK MCP server",
        "no callable bridge",
        "failed: {}; refusing to run tool",
    ]:
        if snippet not in print_handlers:
            fail(failures, f"print_handlers.rs missing fail-closed snippet: {snippet}")

    for test_name in [
        "permission_prompt_denies_when_tool_is_not_advertised",
        "permission_prompt_denies_when_bridge_is_missing",
        "permission_prompt_callback_can_allow_with_updated_input",
        "permission_prompt_callback_errors_fail_closed",
    ]:
        if test_name not in print_handlers:
            fail(failures, f"print_handlers.rs missing test: {test_name}")

    if "create_can_use_tool_with_permission_prompt_callback(" not in print_handlers:
        fail(failures, "default permission prompt helper must delegate to callback-aware path")
    if "create_can_use_tool_with_permission_prompt(permission_prompt_tool_name, sdk_mcp_servers)" not in print_handlers:
        fail(failures, "camelCase helper must use the fail-closed default helper")

    if "wave_w78_permission_prompt_fail_closed_smoke" not in run_all:
        fail(failures, "run_all_smoke.sh must register W78")

    print("=== W78 permission prompt fail-closed smoke ===")
    print(f"print handlers: {PRINT_HANDLERS.relative_to(ROOT)}")
    if failures:
        print("\nFAILURES:")
        for failure in failures:
            print(f" - {failure}")
        return 1
    print("PASS: SDK permission prompt path fails closed unless a callback approves")
    return 0


if __name__ == "__main__":
    sys.exit(main())
