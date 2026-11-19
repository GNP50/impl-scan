#!/usr/bin/env python3
"""Check package inventories and license copies; not a publish dry run."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
PARTIALS = ("registry_enum.hbs", "store.hbs", "dispatch.hbs")

for package in ("impl-scan-attr", "impl-scan"):
    for name in ("LICENSE-MIT", "LICENSE-APACHE"):
        assert (ROOT / name).read_bytes() == (ROOT / package / name).read_bytes(), (package, name)
    listing = subprocess.check_output(
        ["cargo", "package", "--locked", "--list", "--allow-dirty", "-p", package],
        cwd=ROOT,
        text=True,
    ).splitlines()
    for required in ("Cargo.toml", "README.md", "LICENSE-MIT", "LICENSE-APACHE"):
        assert required in listing, (package, required)
    if package == "impl-scan":
        for partial in PARTIALS:
            assert f"src/partials/{partial}" in listing, (package, partial)
    forbidden = (".github/", "target/", "example/", "scripts/")
    assert not any(any(part in path for part in forbidden) for path in listing), (package, listing)
    assert "build.rs" not in listing, (package, "unexpected build script")
    print(f"{package}: {len(listing)} package entries, licenses identical, no build script")
