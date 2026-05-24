#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REMOTE_IO = ROOT / "crates/mossen-cli/src/remote_io.rs"
STRUCTURED_IO = ROOT / "crates/mossen-cli/src/structured_io.rs"
RUN_ALL = ROOT / "scripts/run_all_smoke.sh"


def require(text: str, needle: str, label: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle}")


def main() -> None:
    remote_io = REMOTE_IO.read_text()
    structured_io = STRUCTURED_IO.read_text()
    run_all = RUN_ALL.read_text()

    require(
        structured_io,
        "impl Clone for StructuredIO",
        "StructuredIO clone for async transport bridge",
    )
    require(
        remote_io,
        "mpsc::channel::<String>(256)",
        "RemoteIO inbound queue",
    )
    require(
        remote_io,
        "incoming_tx.try_send(data)",
        "transport data enters inbound queue",
    )
    require(
        remote_io,
        "process_remote_transport_data",
        "RemoteIO inbound processor",
    )
    require(
        remote_io,
        "structured_io.process_line(&line).await",
        "RemoteIO parses inbound data through StructuredIO",
    )
    require(
        remote_io,
        "structured_io.take_outbound_rx().await",
        "RemoteIO owns StructuredIO outbound drain",
    )
    require(
        remote_io,
        "spawn_remote_outbound_drain",
        "RemoteIO outbound transport bridge",
    )
    require(
        remote_io,
        "client.write_event(&value).await",
        "RemoteIO writes outbound messages through CCR when enabled",
    )
    require(
        remote_io,
        "transport.write(&value).await",
        "RemoteIO writes outbound messages through transport fallback",
    )
    require(
        remote_io,
        "close_structured_io.mark_input_closed().await",
        "RemoteIO marks StructuredIO closed on transport close",
    )
    require(
        remote_io,
        "remote_transport_lines_handles_ndjson_and_single_json",
        "RemoteIO line splitting unit coverage",
    )
    require(
        run_all,
        "wave_w129_remote_io_structured_bridge_smoke.py",
        "run_all registration",
    )

    print("wave_w129_remote_io_structured_bridge_smoke: ok")


if __name__ == "__main__":
    main()
