#!/usr/bin/env python3
"""Qualify authored static fixtures only. Never accepts an extractor executable."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

IMAGE = "sha256:7f8acd0cd855b63046cfdf49f21ed319ae6b981758f5fafe2100aed8b4ef21e8"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def run(args, **kwargs):
    result = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120, **kwargs)
    if result.returncode:
        raise RuntimeError(f"command failed {args!r}: {result.returncode}\n{result.stdout!r}\n{result.stderr!r}")
    return result


def events(root):
    data = (root / "products.jsonl").read_bytes()
    result = []
    previous = None
    sequences = {}
    for line in data.splitlines():
        event = json.loads(line)
        assert event["previous_record_sha256"] == previous
        assert event["observer_seq"] == len(result) + 1
        identity = (event["process_generation"], event["logical_thread"])
        assert event["thread_seq"] > sequences.get(identity, 0)
        sequences[identity] = event["thread_seq"]
        assert all(isinstance(parent, int) and 0 < parent < event["observer_seq"] for parent in event["parent_event_ids"])
        previous = sha(line)
        result.append(event)
    marker = json.loads((root / "completion.json").read_text())
    assert marker["count"] == len(result)
    assert marker["stream_sha256"] == sha(data)
    assert marker["last_record_sha256"] == previous
    return result, marker


def blob(root, identity):
    data = (root / identity["blob_ref"]).read_bytes()
    assert len(data) == identity["length"]
    assert sha(len(data).to_bytes(8, "little") + data) == identity["sha256"]
    return data


def file_order(records, root):
    result = []
    # Authored fixture's file operations have no stat timestamps or nondeterministic payloads.
    for event in records:
        if event["kind"] != "syscall_exit" or event["payload"]["number"] not in (0, 1, 3, 8, 257):
            continue
        call = event["payload"]
        number = call["number"]
        args = call["arguments_u64"]
        entry = call["entry_capture"]
        capture = call["capture"]
        item = {"number": number, "result_i64": call["result_i64"], "logical_thread": event["logical_thread"]}
        if "path" in entry:
            item["path"] = entry["path"]
            item["flags_mode"] = args[2:4]
        else:
            item["fd"] = args[0]
        if number in (0, 1):
            item["requested"] = args[2]
        if number == 8:
            item["offset_whence"] = args[1:3]
        for name, source in (("requested_bytes", entry), ("returned_bytes", capture)):
            if name in source:
                item[name] = blob(root, source[name]).hex()
        result.append(item)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--observer", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    source = Path(__file__).resolve().parents[1] / "crates/benilla-extractor-observer/fixtures"
    source_hash = sha((source / "mechanisms.c").read_bytes())
    args.output.mkdir(mode=0o700, parents=True, exist_ok=False)
    identity = run(["docker", "image", "inspect", IMAGE, "--format", "{{.Id}}"])
    assert identity.stdout.strip().decode() == IMAGE
    (args.output / "image-id.txt").write_bytes(identity.stdout)
    invocation = ["docker", "run", "--rm", "--pull=never", "--network", "none", "--read-only", "--mount", f"type=bind,src={source},dst=/src,readonly", "--mount", f"type=bind,src={args.output.resolve()},dst=/out", "-e", "TMPDIR=/out", "-w", "/out", IMAGE, "gcc", "-nostdlib", "-static", "-fno-pie", "-no-pie", "-fno-stack-protector", "-ffreestanding", "-fno-builtin", "-O2", "-g", "-Wl,-e,_start", "/src/mechanisms.c", "-o", "/out/fixture"]
    (args.output / "compile-invocation.json").write_text(json.dumps(invocation, indent=2))
    compiled = subprocess.run(invocation, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120)
    (args.output / "compile.stdout").write_bytes(compiled.stdout)
    (args.output / "compile.stderr").write_bytes(compiled.stderr)
    compiled.check_returncode()
    fixture = args.output.resolve() / "fixture"
    symbols_raw = run(["nm", "-n", str(fixture)]).stdout
    disassembly = run(["objdump", "-w", "-d", str(fixture)]).stdout
    (args.output / "symbols.txt").write_bytes(symbols_raw)
    (args.output / "disassembly.txt").write_bytes(disassembly)
    symbols = {fields[2]: int(fields[0],16) for line in symbols_raw.decode().splitlines() if len(fields := line.split()) == 3}
    instructions = {}
    for line in disassembly.decode().splitlines():
        match = re.match(r"\s*([0-9a-f]+):\s*((?:[0-9a-f]{2} )+)\s*",line)
        if match:
            instructions[int(match[1],16)] = bytes.fromhex(match[2])
    def gate(name, end=None, preserve=True):
        pc = symbols[name]
        window = None
        if end:
            stop = symbols[end]
            window = {"end_pc":stop,"max_steps":32,"preserve_fp":preserve,"instructions":[{"pc":address,"bytes":list(data)} for address,data in instructions.items() if pc <= address < stop]}
        return {"site_id":name,"pc":pc,"original_bytes":list(instructions[pc]),"window":window}
    fixture_hash = sha(fixture.read_bytes())
    observer_hash = sha(args.observer.read_bytes())
    inputs = {str(path.name):sha(path.read_bytes()) for path in (source.parent/"src").glob("*.rs")}
    inputs["qualification_script"] = sha(Path(__file__).read_bytes())
    inputs["Cargo.lock"] = sha((source.parents[2]/"Cargo.lock").read_bytes())
    (args.output/"observer-inputs.json").write_text(json.dumps(inputs,indent=2)+"\n")
    report = {"image":IMAGE,"fixture_sha256":fixture_hash,"fixture_source_sha256":sha((source/"mechanisms.c").read_bytes()),"observer_sha256":observer_hash,"qualifications":[]}
    environment = [list(b"LC_ALL=C"),list(b"DUP=one"),list(b"DUP=two"),list(b"RAW=\xff")]
    for mode in ("normal","fork","thread", *(f"numeric-{profile:03}" for profile in range(71))):
        raw_outputs = []
        orders = []
        exit_states = []
        gate_orders = []
        # A and C omit semantic gates. B uses the same syscall witness plus hardware/step probes.
        for level, name in (("kernel-witness","A"),("full","B"),("kernel-witness","C")):
            attempt = args.output / f"{mode}-{name}"
            attempt.mkdir()
            products = attempt / "products"
            products.mkdir()
            # Same exact path in every execve vector, byte-preserving env including duplicates.
            vector = [list(b"fixture"),list(mode.encode())]
            if mode.startswith("numeric-"):
                vector.append(list(str(int(mode.split("-")[1])).encode()))
            plan = {"attempt_id":f"{mode}-{name}","stage":"authored-fixture","executable":list(bytes(fixture)),"executable_sha256":fixture_hash,"argv":vector,"env":environment,"cwd":list(bytes(products.resolve())),"level":level,"gates":[gate("gate_one","gate_end"),gate("gate_two")],"max_events":100000,"max_blob_bytes":64*1024*1024}
            path = attempt / "plan.json"
            path.write_text(json.dumps(plan))
            result = subprocess.run([str(args.observer.resolve()),str(path),str(attempt/"receipts")],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=120)
            (attempt/"observer.stdout").write_bytes(result.stdout)
            (attempt/"observer.stderr").write_bytes(result.stderr)
            result.check_returncode()
            records,marker = events(attempt/"receipts")
            assert marker["clean_completion"]
            outputs = {str(p.relative_to(products)):p.read_bytes().hex() for p in products.rglob("*") if p.is_file()}
            raw_outputs.append(outputs)
            orders.append(file_order(records,attempt/"receipts"))
            exit_states.append([(e["logical_thread"],blob(attempt/"receipts",e["payload"]["xsave"]).hex()) for e in records if e["kind"] == "exit_state"])
            gate_orders.append([e["source_site_id"] for e in records if e["kind"] == "hardware_breakpoint"])
            if level == "full":
                expected = ["gate_one","gate_two","gate_one"] + (["gate_two"] if mode == "fork" else [])
                assert gate_orders[-1] == expected, gate_orders[-1]
                assert sum(e["kind"] == "native_step" for e in records) == 6
                if mode in ("fork","thread"):
                    assert sum(e["kind"] == "child_created" for e in records) == 1
                    assert sum(e["kind"] == "thread_birth" for e in records) == 1
        assert raw_outputs[0] == raw_outputs[1] == raw_outputs[2], mode
        assert orders[0] == orders[1] == orders[2], mode
        # Child exit order is not an invented causal order. Compare per logical thread.
        assert sorted(exit_states[0]) == sorted(exit_states[1]) == sorted(exit_states[2]), mode
        report["qualifications"].append({"mode":mode,"byte_equal":True,"kernel_file_order_equal":True,"exit_xsave_equal":True,"gate_order":gate_orders[1],"file_operations":len(orders[0])})
    for name, mode, gates, ceiling, expected in (
        ("perturb","perturb",[gate("perturb","perturb_end")],100000,"floating_point_changed"),
        ("unsupported","normal",[{**gate("gate_one","gate_end"),"window":{"end_pc":symbols["gate_end"],"max_steps":32,"preserve_fp":True,"instructions":[{"pc":symbols["gate_one"],"bytes":list(instructions[symbols["gate_one"]])}]}}],100000,"unsupported_instruction"),
        ("syscall","unsupported-syscall",[],100000,"unsupported_syscall"),
        ("shared","shared",[],100000,"shared_memory_ownership"),
        ("ceiling","normal",[],3,"resource_ceiling"),
        ("bytes","normal",[{**gate("gate_one"),"original_bytes":[0x91]}],100000,"identity_changed"),
        ("implicit","leaked",[],100000,"unsupported_syscall"),
    ):
        attempt = args.output / name
        attempt.mkdir()
        plan = {"attempt_id":name,"stage":"authored-fixture","executable":list(bytes(fixture)),"executable_sha256":fixture_hash,"argv":[list(b"fixture"),list(mode.encode())],"env":environment,"cwd":list(bytes(attempt.resolve())),"level":"full","gates":gates,"max_events":ceiling,"max_blob_bytes":64*1024*1024}
        path = attempt / "plan.json"
        path.write_text(json.dumps(plan))
        result = subprocess.run([str(args.observer.resolve()),str(path),str(attempt/"receipts")],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=120)
        (attempt/"observer.stdout").write_bytes(result.stdout)
        (attempt/"observer.stderr").write_bytes(result.stderr)
        assert result.returncode == 1
        records,marker = events(attempt/"receipts")
        assert not marker["clean_completion"]
        assert records[-1]["kind"] == "refusal"
        assert records[-1]["payload"]["first_failure"]["kind"] == expected, records[-1]
        report["qualifications"].append({"mode":name,"typed_refusal":expected})
    assert sha((source / "mechanisms.c").read_bytes()) == source_hash
    assert sha(args.observer.read_bytes()) == observer_hash
    assert sha(fixture.read_bytes()) == fixture_hash
    (args.output / "qualification.json").write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps(report,indent=2))


if __name__ == "__main__":
    main()
