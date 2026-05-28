#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODEL = ROOT / "crates/mossen-tui/src/render_model.rs"
CONTRACT = ROOT / "crates/mossen-tui/tests/render_contract.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    model = MODEL.read_text()
    contract = CONTRACT.read_text()
    run_all = RUN_ALL.read_text()

    require(model, "ch.is_control()", "semantic control character scrubber")
    require(model, '"authorizationheader"', "authorization header redaction")
    require(model, '"privatekey"', "private key redaction")
    require(model, "token_like_secret", "token-shaped secret redaction")
    require(
        model,
        "arbitrary_json_tool_payloads_scrub_controls_and_redact_token_secrets",
        "render model arbitrary payload regression",
    )
    require(contract, "fn arbitrary_tool_payload_app", "arbitrary payload app fixture")
    require(
        contract,
        "app_render_contract_arbitrary_tool_payloads_are_scrubbed_and_redacted",
        "product arbitrary payload contract",
    )
    require(contract, "raw-result-session-token", "session token negative assertion")
    require(contract, "arbitrary-tool-visible-nested", "visible nested payload assertion")
    require(
        run_all,
        "wave_w98_render_arbitrary_payload_fuzz_smoke.py",
        "run_all registration",
    )

    print("wave_w98_render_arbitrary_payload_fuzz_smoke: ok")


if __name__ == "__main__":
    main()
