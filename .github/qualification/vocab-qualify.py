"""Private fork qualification, never shipped in the application PR."""
from pathlib import Path
import hashlib
import json
import os
import re
import subprocess
import sys
import xml.etree.ElementTree as ET
import tomllib

APP = Path(sys.argv[1]).resolve()
OUT = Path(sys.argv[2]).resolve()
OUT.mkdir(parents=True, exist_ok=True)
HEAD = "8b260ee746750e48a43c431acfac124d308ac788"
TREE = "0b7283cca72800f82762f4afcd3f0460b70d51bb"
BASE = "dfb09bf815326913d7d7822fa7228f2602fb4976"
records = []
(OUT / "started.json").write_text(json.dumps({"head": HEAD, "tree": TREE, "platform": os.name, "state": "qualification started; no gates passed yet"}, indent=2))


def git(*args):
    return subprocess.check_output(["git", *args], cwd=APP)


def tracked_blob_bytes(path, mode, windows=(os.name == "nt")):
    if mode == b"120000" and path.is_symlink():
        target = os.readlink(path)
        # Windows reports a native symlink's relative target using backslashes.
        # Git's stored target spelling uses forward slashes. Normalize only
        # this OS representation, never regular source files or symlink text
        # materializations, and never resolve/dereference a target.
        if windows:
            target = target.replace("\\", "/")
        return os.fsencode(target)
    return path.read_bytes()


def freeze(label):
    assert git("rev-parse", "HEAD").decode().strip() == HEAD
    assert git("rev-parse", "HEAD^{tree}").decode().strip() == TREE
    count = 0
    for record in git("ls-tree", "-rz", "HEAD").split(b"\0"):
        if not record:
            continue
        info, raw_path = record.split(b"\t", 1)
        mode, kind, expected = info.split()
        path = APP / os.fsdecode(raw_path)
        if kind != b"blob":
            raise AssertionError("Unexpected non-blob tracked entry: " + str(path))
        data = tracked_blob_bytes(path, mode)
        actual = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
        assert actual == expected.decode(), (label, str(path), actual, expected.decode())
        count += 1
    records.append({"freeze": label, "tracked_blobs": count, "head": HEAD, "tree": TREE})
    (OUT / "receipt.json").write_text(json.dumps(records, indent=2))


def run(name, argv, cwd=APP, expected=0):
    result = subprocess.run(argv, cwd=cwd, capture_output=True, text=True, timeout=1200)
    (OUT / (name + ".log")).write_text(result.stdout + result.stderr, encoding="utf-8")
    records.append({"command": argv, "cwd": str(cwd), "exit": result.returncode, "log": name + ".log"})
    (OUT / "receipt.json").write_text(json.dumps(records, indent=2))
    if expected is not None:
        assert result.returncode == expected, (name, result.returncode)
    return result


def cases(path):
    return list(ET.parse(path).getroot().iter("testcase"))


def main():
    provider_keys = [
        "OPENAI_API_KEY", "ANTHROPIC_API_KEY", "GEMINI_API_KEY", "GOOGLE_API_KEY",
        "HF_TOKEN", "HUGGING_FACE_HUB_TOKEN", "OPENROUTER_API_KEY",
        "AZURE_OPENAI_API_KEY", "COHERE_API_KEY", "MISTRAL_API_KEY",
        "DEEPSEEK_API_KEY", "GROQ_API_KEY",
    ]
    assert not any(os.environ.get(key) for key in provider_keys), "Unexpected provider credential present"
    assert not (APP / ".env").exists(), "Unexpected application dotenv file"
    assert tomllib.loads((APP / "Cargo.toml").read_text(encoding="utf-8"))["features"]["default"] == []
    records.append({"provider_credentials": "absent", "dotenv": "absent", "cargo_features": "default empty", "network_scope": "Package/toolchain/advisory downloads permitted; no OS-wide network isolation claimed. Default tests inspected for provider/model operations; embeddings-feature tests are disabled."})
    freeze("before dependencies")
    run("python-install", [sys.executable, "-m", "pip", "install", "-e", ".[dev]"], cwd=APP / "python")
    freeze("after dependencies")
    run("python-version", [sys.executable, "-c", "import sys,pyoxigraph,pytest;print(sys.version);print(pyoxigraph.__version__);print(pytest.__version__)"])
    run("rust-version", ["rustc", "-Vv"])
    run("cargo-version", ["cargo", "--version"])
    env = dict(os.environ)
    env["PYTHONPATH"] = str(APP / "python/src")
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    # The editable wheel permits subprocess tests to find pyoxigraph without
    # adding any dependency to the project or changing global Python state.
    os.environ.update({"PYTHONPATH": env["PYTHONPATH"], "PYTHONDONTWRITEBYTECODE": "1"})
    paths = ["src/vocab_check.rs", "python/src/open_ontologies_lite/vocab_check.py"]
    original = {path: (APP / path).read_bytes() for path in paths}
    try:
        for path in paths:
            (APP / path).write_bytes(git("show", BASE + ":" + path))
        before_xml = OUT / "python-before.xml"
        run("python-before", [sys.executable, "-m", "pytest", "python/tests/test_vocab_property_characteristics.py", "-q", "--junitxml=" + str(before_xml)], expected=1)
        found = cases(before_xml)
        failures = {case.get("name") for case in found if case.find("failure") is not None}
        expected = {"test_characteristic_declares_property_without_object_property_repetition[" + kind + "]" for kind in ["TransitiveProperty", "SymmetricProperty", "AsymmetricProperty", "ReflexiveProperty", "IrreflexiveProperty"]}
        assert failures == expected
        assert len(found) == 6
        assert all(case.find("error") is None and case.find("skipped") is None for case in found)
        assert all("AssertionError" in case.find("failure").get("message", "") for case in found if case.find("failure") is not None)
        rust = run("rust-before", ["cargo", "test", "--locked", "--test", "vocab_property_characteristics_test", "--", "--exact", "characteristic_only_properties_are_declared_vocabulary", "--nocapture"], expected=101)
        text = rust.stdout + rust.stderr
        assert "running 1 test" in text
        assert "test characteristic_only_properties_are_declared_vocabulary ... FAILED" in text
        assert re.search(r"test result: FAILED\. 0 passed; 1 failed;", text)
        assert "TransitiveProperty" in text and "assertion" in text and "failed" in text
    finally:
        for path, data in original.items():
            (APP / path).write_bytes(data)
    freeze("restored after causal intervention")
    focused_xml = OUT / "python-focused.xml"
    run("python-focused", [sys.executable, "-m", "pytest", "python/tests/test_vocab_check.py", "python/tests/test_vocab_property_characteristics.py", "-q", "--junitxml=" + str(focused_xml)])
    found = cases(focused_xml)
    assert len(found) == 14
    assert all(not any(case.find(tag) is not None for tag in ["failure", "error", "skipped"]) for case in found)
    run("rust-focused", ["cargo", "test", "--locked", "--test", "vocab_property_characteristics_test", "--", "--nocapture"])
    run("python-core", [sys.executable, "-m", "pytest", "python/tests", "--ignore=python/tests/test_align.py", "--ignore=python/tests/test_shacl.py", "--ignore=python/tests/test_shacl_vacuous.py", "-q", "--junitxml=" + str(OUT / "python-core.xml")])
    run("cargo-build", ["cargo", "build", "--locked"])
    run("cargo-test", ["cargo", "test", "--locked", "--", "--nocapture"])
    run("cargo-clippy", ["cargo", "clippy", "--locked", "--", "-D", "warnings"])
    run("audit-install", ["cargo", "install", "cargo-audit", "--locked"])
    run("audit-version", ["cargo", "audit", "--version"])
    run("cargo-audit", ["cargo", "audit"])
    freeze("all qualification gates completed")


if __name__ == "__main__":
    try:
        main()
    finally:
        freeze("final always guard")
