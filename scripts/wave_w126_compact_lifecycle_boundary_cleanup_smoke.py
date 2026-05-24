#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
COMPACT = ROOT / "crates/mossen-agent/src/services/compact/compact.rs"
POST_CLEANUP = ROOT / "crates/mossen-agent/src/services/compact/post_compact_cleanup.rs"
AUTO_COMPACT = ROOT / "crates/mossen-agent/src/services/compact/auto_compact.rs"
CONTEXT = ROOT / "crates/mossen-agent/src/context/mod.rs"
APP = ROOT / "crates/mossen-tui/src/app.rs"
KEYBINDING = ROOT / "crates/mossen-tui/tests/keybinding_smoke.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    compact = COMPACT.read_text()
    post_cleanup = POST_CLEANUP.read_text()
    auto_compact = AUTO_COMPACT.read_text()
    context = CONTEXT.read_text()
    app = APP.read_text()
    keybinding = KEYBINDING.read_text()
    run_all = RUN_ALL.read_text()

    require(
        compact,
        "pub fn prepend_compact_boundary_to_messages(",
        "shared compact boundary prepend helper",
    )
    require(
        compact,
        "\"compact_metadata\".to_string()",
        "compact boundary metadata",
    )
    require(
        compact,
        "prepend_compact_boundary_adds_metadata_and_recomputes_tokens",
        "compact boundary unit coverage",
    )
    require(
        post_cleanup,
        "suppress_compact_warning();",
        "post compact cleanup suppresses warning",
    )
    require(
        post_cleanup,
        "PostCompactCleanupOutcome",
        "post compact cleanup reports lifecycle effects",
    )
    require(
        context,
        "compact_warning_state::clear_compact_warning_suppression();",
        "auto compact clears warning suppression on new attempt",
    )
    require(
        context,
        "post_compact_cleanup::run_post_compact_cleanup(None)",
        "context auto compact runs post cleanup",
    )
    require(
        auto_compact,
        "post_compact_cleanup::run_post_compact_cleanup(query_source)",
        "service auto compact runs post cleanup",
    )
    require(
        app,
        "prepend_compact_boundary_to_messages(",
        "manual compact writes boundary into engine history",
    )
    require(
        app,
        "post_compact_cleanup::run_post_compact_cleanup(Some(",
        "manual compact runs post cleanup",
    )
    require(
        keybinding,
        "manual compact boundary should carry metadata",
        "manual compact boundary keybinding coverage",
    )
    require(
        run_all,
        "wave_w126_compact_lifecycle_boundary_cleanup_smoke.py",
        "run_all registration",
    )

    print("wave_w126_compact_lifecycle_boundary_cleanup_smoke: ok")


if __name__ == "__main__":
    main()
