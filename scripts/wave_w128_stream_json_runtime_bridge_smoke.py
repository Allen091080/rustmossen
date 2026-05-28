#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPL = ROOT / "crates/mossen-cli/src/repl.rs"
MAIN = ROOT / "crates/mossen-cli/src/main.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    repl = REPL.read_text()
    main_rs = MAIN.read_text()
    run_all = RUN_ALL.read_text()

    require(
        repl,
        "pub async fn run_oneshot_stream_json",
        "stream-json oneshot runtime entrypoint",
    )
    require(
        repl,
        "build_oneshot_prompt_params",
        "shared oneshot prompt setup",
    )
    require(
        repl,
        "StructuredIO::new(false)",
        "stream-json runtime creates StructuredIO",
    )
    require(
        repl,
        ".take_outbound_rx()",
        "stream-json runtime drains StructuredIO outbound messages",
    )
    require(
        repl,
        "stdin_io.process_line(&line).await",
        "stream-json runtime processes stdin control lines",
    )
    require(
        repl,
        "StdoutMessage::StreamEvent(value)",
        "stream-json runtime emits SDK messages through the NDJSON drain",
    )
    require(
        repl,
        "SdkMessage::Result",
        "stream-json runtime preserves terminal result messages",
    )
    require(
        main_rs,
        "run_oneshot_stream_json",
        "CLI route imports stream-json runtime",
    )
    require(
        main_rs,
        "matches!(&cli.emit, EmitFormat::StreamJson)",
        "CLI route selects streaming runtime for --emit stream-json",
    )
    require(
        run_all,
        "wave_w128_stream_json_runtime_bridge_smoke.py",
        "run_all registration",
    )

    print("wave_w128_stream_json_runtime_bridge_smoke: ok")


if __name__ == "__main__":
    main()
