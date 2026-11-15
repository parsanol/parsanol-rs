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


def expect_in(name, wanted, versions):
    if wanted not in versions:
        failures.append(f"{name}: registry does not serve {wanted!r}")


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

# npm / rubygems / pypi: the manifest's runtime version EXISTS on the
# registry. The runtime versions follow the engine-linked scheme
# ({engine-semver}.{runtime-digit}; npm: the engine semver) -- a scheme
# change's legacy-line versions can sort ABOVE the new scheme forever
# (e.g. rubygems 1.3.151 > 0.20.23.0), so a max/latest comparison is
# wrong here: what the train guarantees is that the manifest's version
# is published, and the `latest` caveat for bare installs is documented
# in the manifest comment.
npm_versions = list(get("https://registry.npmjs.org/parsanol")["versions"])
expect_in("npm", MANIFEST["npm"], npm_versions)

gems = get("https://rubygems.org/api/v1/versions/parsanol.json")
expect_in("rubygems", MANIFEST["rubygems"], [g["number"] for g in gems])

pypi_versions = list(get("https://pypi.org/pypi/parsanol/json")["releases"])
expect_in("pypi", MANIFEST["pypi"], pypi_versions)

if failures:
    print("\n".join(failures), file=sys.stderr)
    sys.exit(f"{len(failures)} engine-compat drift(s) -- update ENGINE-COMPAT.json with the train")
print(f"engine-compat: manifest matches all registries "
      f"(crates {MANIFEST['crates']}, npm {MANIFEST['npm']}, "
      f"rubygems {MANIFEST['rubygems']}, pypi {MANIFEST['pypi']})")
