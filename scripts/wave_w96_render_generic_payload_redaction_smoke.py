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

    require(model, "fn semantic_object_field_text", "semantic JSON field renderer")
    require(model, "fn is_sensitive_json_key", "sensitive JSON key detector")
    require(model, '"apikey"', "API key redaction matcher")
    require(model, '"secrettoken"', "secret token redaction matcher")
    require(model, '"password"', "password redaction matcher")
    require(
        model,
        "generic_json_tool_payload_redacts_sensitive_fields",
        "render model redaction regression",
    )
    require(model, "token_count: 1234", "non-secret token count assertion")
    require(
        contract,
        "app_render_contract_generic_json_tool_payload_redacts_secrets",
        "product render redaction contract",
    )
    require(contract, "raw-api-secret", "API secret negative assertion")
    require(contract, "raw-nested-secret", "nested secret negative assertion")
    require(contract, "raw-array-secret", "array secret negative assertion")
    require(
        run_all,
        "wave_w96_render_generic_payload_redaction_smoke.py",
        "run_all registration",
    )

    print("wave_w96_render_generic_payload_redaction_smoke: ok")


if __name__ == "__main__":
    main()
