#!/usr/bin/env python3
"""Package only CI's tested bindgen runtime, with source/build provenance."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
RUNTIME = ("parsanol.js", "parsanol.d.ts", "parsanol_bg.wasm", "parsanol_bg.wasm.d.ts")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True, timeout=30).strip()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--web-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--source-head", required=True)
    parser.add_argument("--bindgen", required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{40}", args.source_head):
        parser.error("source head must be an exact commit SHA")
    for name in RUNTIME:
        if not (args.web_dir / name).is_file():
            parser.error(f"missing bindgen output: {name}")
    # Fresh output only: never overwrite another build or sweep a workspace.
    args.output_dir.mkdir(parents=True, exist_ok=False)
    output = args.output_dir.resolve()
    clean = not command("git", "status", "--porcelain", "--untracked-files=no")
    if os.environ.get("GITHUB_ACTIONS") == "true" and not clean:
        raise RuntimeError("CI runtime must come from a clean tracked checkout")
    version = re.search(r'^version\s*=\s*"([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.M).group(1)
    provenance = {
        "schema": "parsanol-wasm-ci/v1",
        "kind": "ci-test-build-not-release",
        "repository": "https://github.com/parsanol/parsanol-rs",
        "source_head_sha": args.source_head,
        "checkout_build_sha": command("git", "rev-parse", "HEAD"),
        "tracked_worktree_clean": clean,
        "cargo_lock_sha256": digest(ROOT / "Cargo.lock"),
        "workflow": os.environ.get("GITHUB_WORKFLOW"),
        "run_id": os.environ.get("GITHUB_RUN_ID"),
        "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
        "target": "wasm32-unknown-unknown",
        "bindgen_target": "web",
        "profile": "release",
        "versions": {
            "cargo": command("cargo", "--version"),
            "rustc": command("rustc", "--version"),
            "wasm_bindgen": command(args.bindgen, "--version"),
            "node": command("node", "--version"),
            "npm": command("npm", "--version"),
        },
    }
    with tempfile.TemporaryDirectory(prefix="parsanol-wasm-pack-") as temporary:
        stage = Path(temporary)
        for name in RUNTIME:
            shutil.copyfile(args.web_dir / name, stage / name)
        shutil.copyfile(ROOT / "LICENSE", stage / "LICENSE")
        shutil.copyfile(ROOT / "tools/wasm-README.md", stage / "README.md")
        files = [*RUNTIME, "LICENSE", "README.md", "build-provenance.json", "SHA256SUMS"]
        write_json(stage / "package.json", {
            "name": "@parsanol/wasm", "version": version, "private": True,
            "description": "Parsanol tested CI WebAssembly runtime (not a release)",
            "license": "MIT", "type": "module", "files": files,
            "module": "parsanol.js", "types": "parsanol.d.ts",
            "exports": {".": {"types": "./parsanol.d.ts", "default": "./parsanol.js"},
                        "./parsanol_bg.wasm": "./parsanol_bg.wasm",
                        "./package.json": "./package.json"},
        })
        write_json(stage / "build-provenance.json", provenance)
        hashes = {path.name: digest(path) for path in sorted(stage.iterdir())}
        (stage / "SHA256SUMS").write_text("".join(f"{sha}  {name}\n" for name, sha in hashes.items()))
        env = dict(os.environ, npm_config_cache=str(stage / "npm-cache"))
        packed = subprocess.check_output(
            ["npm", "pack", "--offline", "--ignore-scripts", "--json", "--pack-destination", str(output)],
            cwd=stage, env=env, text=True, timeout=60,
        )
        pack = json.loads(packed)[0]
        archive = output / pack["filename"]
        expected = set(hashes) | {"SHA256SUMS"}
        # Verify and extract the actual packed bytes that Node/browser checks
        # consume. No symlinks, scripts, caches, context or unknown files ship.
        package = output / "package"
        package.mkdir()
        with tarfile.open(archive) as tar:
            members = tar.getmembers()
            if {m.name for m in members} != {"package/" + name for name in expected}:
                raise RuntimeError("unexpected npm package surface")
            for member in members:
                if not member.isfile():
                    raise RuntimeError("runtime package must contain only regular files")
                name = member.name.removeprefix("package/")
                (package / name).write_bytes(tar.extractfile(member).read())
        for name, sha in hashes.items():
            if digest(package / name) != sha:
                raise RuntimeError(f"packed file digest mismatch: {name}")
        payload = {path.relative_to(output).as_posix(): digest(path)
                   for path in sorted(output.rglob("*")) if path.is_file()}
        write_json(output / "artifact.json", {
            "schema": "parsanol-wasm-ci/v1", "source_head_sha": args.source_head,
            "checkout_build_sha": provenance["checkout_build_sha"],
            "npm_archive": archive.name, "sha256": payload,
        })
        payload["artifact.json"] = digest(output / "artifact.json")
        (output / "SHA256SUMS").write_text("".join(f"{sha}  {name}\n" for name, sha in sorted(payload.items())))
        print(json.dumps({"archive": archive.name, "archive_sha256": digest(archive),
                          "package_files": sorted(expected), "output": str(output)}))


if __name__ == "__main__":
    main()
