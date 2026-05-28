#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "crates/mossen-tui/src/app.rs"
EVENTS = ROOT / "crates/mossen-tui/src/render_events.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    app = APP.read_text()
    events = EVENTS.read_text()
    run_all = RUN_ALL.read_text()

    require(events, "pub const STREAM_THROTTLE_MS", "exported stream throttle constant")
    require(app, "RenderRefreshPolicy", "render refresh policy import/use")
    require(app, "fn mark_render_dirty_for_refresh", "policy-aware dirty marker")
    require(app, "fn mark_render_dirty_throttled", "throttled dirty marker")
    require(app, "render_throttled_dirty_at", "throttled redraw deadline state")
    require(app, "throttled_due_in_ms", "throttled redraw scheduler diagnostics")
    require(app, "next_frame_due_in_ms", "next frame scheduler diagnostics")
    require(app, "fn next_render_frame_due_at", "next frame deadline calculation")
    require(app, "tokio::time::sleep_until", "deadline-driven render wakeup")
    require(app, "fn note_transcript_changed_for_refresh", "policy-aware transcript mutation")
    require(
        app,
        "note_transcript_changed_for_refresh(\n                            Self::streaming_render_refresh_policy(),\n                        );",
        "streaming text/thinking transcript throttling",
    )
    require(
        app,
        "streaming_text_delta_dirty_mark_is_throttled",
        "streaming dirty throttle regression test",
    )

    tail = app[app.find("pub fn handle_engine_message") : app.find("fn prepare_render_turn_for_engine_message")]
    if "        self.mark_render_dirty();\n    }\n\n    fn prepare_render_turn_for_engine_message" in tail:
        raise SystemExit("handle_engine_message still ends with unconditional dirty marking")

    if "self.handle_engine_message(m);\n                                    self.mark_render_dirty();" in app:
        raise SystemExit("run loop still forces dirty after every engine message")

    require(
        app,
        "throttled_streaming_delta_gets_paced_followup_frame",
        "paced follow-up frame regression test",
    )
    require(
        app,
        "next_render_frame_deadline_prefers_throttled_streaming_updates",
        "next frame deadline regression test",
    )

    require(
        run_all,
        "wave_w80_render_stream_throttle_smoke.py",
        "run_all registration",
    )

    print("wave_w80_render_stream_throttle_smoke: ok")


if __name__ == "__main__":
    main()
