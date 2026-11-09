#!/usr/bin/env python3
"""ruby#421 -- verify ENGINE-COMPAT.json against the live registries.

The manifest maps one engine commit to the package versions that carry
it across the four registries. This check keeps it honest: every field
must match what the registry actually serves, and the crates version's
release tag must point at the manifest's engine commit. A train that
rides without updating the manifest fails here.

    python3 tools/check_engine_compat.py
"""
import json
import os
import sys
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MANIFEST = json.load(open(os.path.join(ROOT, "ENGINE-COMPAT.json")))


def get(url):
    request = urllib.request.Request(url, headers={"User-Agent": "parsanol-compat-check"})
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


failures = []


def expect(name, actual, wanted):
    if actual != wanted:
        failures.append(f"{name}: registry serves {actual!r}, manifest says {wanted!r}")


# crates.io: the manifest's crates version is the max published
crate = get("https://crates.io/api/v1/crates/parsanol")["crate"]["max_version"]
expect("crates", crate, MANIFEST["crates"])

# the release tag for that version points at the engine commit
tag = get("https://api.github.com/repos/parsanol/parsanol-rs/git/ref/tags/"
          f"parsanol-v{MANIFEST['crates']}")["object"]
commit = tag["sha"]
if tag["type"] == "tag":
    # annotated tag: the ref names the tag object, not the commit
    commit = get(f"https://api.github.com/repos/parsanol/parsanol-rs/git/tags/{commit}")["object"]["sha"]
if commit != MANIFEST["engine_commit"]:
    failures.append(f"tag parsanol-v{MANIFEST['crates']}: points at {commit[:12]}, "
                    f"manifest says {MANIFEST['engine_commit'][:12]}")

# npm: the parsanol package's latest is the manifest's npm version
npm = get("https://registry.npmjs.org/parsanol")["dist-tags"]["latest"]
expect("npm", npm, MANIFEST["npm"])

# rubygems: the parsanol gem's latest is the manifest's rubygems version
gems = get("https://rubygems.org/api/v1/versions/parsanol/latest.json")
expect("rubygems", gems["version"], MANIFEST["rubygems"])

# pypi: the parsanol package's latest is the manifest's pypi version
pypi = get("https://pypi.org/pypi/parsanol/json")["info"]["version"]
expect("pypi", pypi, MANIFEST["pypi"])

if failures:
    print("\n".join(failures), file=sys.stderr)
    sys.exit(f"{len(failures)} engine-compat drift(s) -- update ENGINE-COMPAT.json with the train")
print(f"engine-compat: manifest matches all registries "
      f"(crates {MANIFEST['crates']}, npm {MANIFEST['npm']}, "
      f"rubygems {MANIFEST['rubygems']}, pypi {MANIFEST['pypi']})")
