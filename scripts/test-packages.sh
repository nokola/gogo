#!/bin/sh
# Packs each crate under crates/ the way `cargo publish` does, unpacks the
# packages, and runs `cargo test` in each one.
#
# Needed because a crates.io package carries one crate's folder and
# nothing around it. A test that reads `docs/` or `assets/` passes in the
# repository and fails for whoever downloads the crate, and nothing else
# notices: `cargo publish` builds the library and runs no test.
#
# Usage, from anywhere:  sh scripts/test-packages.sh   (needs `tar`)
#
# The packages are made from the files as they are now, committed or not.
# They are unpacked into target/package-tests; the build goes to
# target/package-tests-build, which is kept from one run to the next.
set -eu
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
names=$(for dir in crates/*/; do basename "$dir"; done)

packages=""
for name in $names; do
    packages="$packages -p $name"
done
# shellcheck disable=SC2086  # the list is split into words on purpose
cargo package $packages --no-verify --allow-dirty

out=target/package-tests
rm -rf "$out"
mkdir -p "$out/.cargo"
# Each package asks crates.io for the crates it needs. The ones made here
# are answered from the copies beside it, so that a version that is not
# published yet can be tested.
echo "[patch.crates-io]" > "$out/.cargo/config.toml"
for name in $names; do
    tar -xzf "target/package/$name-$version.crate" -C "$out"
    echo "$name = { path = \"$name-$version\" }" >> "$out/.cargo/config.toml"
done

CARGO_TARGET_DIR="$(pwd)/target/package-tests-build"
export CARGO_TARGET_DIR
for name in $names; do
    echo "=== cargo test on the package of $name $version"
    (cd "$out/$name-$version" && cargo test)
done
echo "=== the package of every crate passes its own tests: $(echo $names)"
