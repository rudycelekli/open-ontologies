#!/usr/bin/env bash
set -euo pipefail
expected_tree=ea8e4363436be7ff2ced3223992e8d5864795b3d
base=fec2b4713bbab52596adbde55decd862e443b020
test "$(git rev-parse HEAD^{tree})" = "$expected_tree"
test "$(git rev-parse HEAD^)" = "$base"
restore_source() {
  git restore --source=HEAD src/vocab_check.rs python/src/open_ontologies_lite/vocab_check.py
}
trap restore_source EXIT
# Both real source filters are restored temporarily; new regression tests stay present.
git show "$base:src/vocab_check.rs" > src/vocab_check.rs
git show "$base:python/src/open_ontologies_lite/vocab_check.py" > python/src/open_ontologies_lite/vocab_check.py
set +e
cargo test --locked --offline --test vocab_characteristic_property_test -- --nocapture > "$RUNNER_TEMP/native-before.log" 2>&1
native_status=$?
python -m pytest python/tests/test_vocab_check.py -q > "$RUNNER_TEMP/python-before.log" 2>&1
python_status=$?
set -e
cat "$RUNNER_TEMP/native-before.log" "$RUNNER_TEMP/python-before.log"
test "$native_status" = 101
grep -q 'assertion.*failed' "$RUNNER_TEMP/native-before.log"
grep -q 'characteristic_properties_declare_vocabulary_without_redundant_types.*FAILED' "$RUNNER_TEMP/native-before.log"
test "$python_status" = 1
grep -q '4 failed, 8 passed' "$RUNNER_TEMP/python-before.log"
git restore --source=HEAD src/vocab_check.rs python/src/open_ontologies_lite/vocab_check.py
test "$(git write-tree)" = "$expected_tree"
git diff --exit-code
rustfmt --edition 2024 --check tests/vocab_characteristic_property_test.rs
set +e
cargo fmt --all -- --check > "$RUNNER_TEMP/fmt-fixed.log" 2>&1
fmt_fixed=$?
set -e
cat "$RUNNER_TEMP/fmt-fixed.log"
if test "$fmt_fixed" != 0; then
  mkdir -p "$RUNNER_TEMP/format-base"
  git archive "$base" | tar -x -C "$RUNNER_TEMP/format-base"
  set +e
  (cd "$RUNNER_TEMP/format-base" && cargo fmt --all -- --check) > "$RUNNER_TEMP/fmt-base.log" 2>&1
  fmt_base=$?
  set -e
  cat "$RUNNER_TEMP/fmt-base.log"
  test "$fmt_base" != 0
  sed "s|$PWD|SOURCE|g" "$RUNNER_TEMP/fmt-fixed.log" > "$RUNNER_TEMP/fmt-fixed-normalized.log"
  sed "s|$RUNNER_TEMP/format-base|SOURCE|g" "$RUNNER_TEMP/fmt-base.log" > "$RUNNER_TEMP/fmt-base-normalized.log"
  diff -u "$RUNNER_TEMP/fmt-base-normalized.log" "$RUNNER_TEMP/fmt-fixed-normalized.log"
  echo 'Full formatting fails identically on exact base; no introduced formatting debt.'
fi
cargo test --locked --offline --test vocab_characteristic_property_test -- --nocapture
python -m pytest python/tests/test_vocab_check.py -q
cargo build --locked --offline
cargo test --locked --offline -- --nocapture
cargo clippy --locked --offline -- -D warnings
python -m pytest python/tests -q
python -m build --no-isolation --wheel python --outdir "$RUNNER_TEMP/wheels"
python -m venv "$RUNNER_TEMP/wheel-consumer"
"$RUNNER_TEMP/wheel-consumer/bin/pip" install --no-index --find-links "$RUNNER_TEMP/python-wheels" "$RUNNER_TEMP"/wheels/*.whl
(cd "$RUNNER_TEMP" && "$RUNNER_TEMP/wheel-consumer/bin/python" -c 'from open_ontologies_lite import vocab_check; assert vocab_check("@prefix owl: <http://www.w3.org/2002/07/owl#> . <http://ex/p> a owl:TransitiveProperty .", "<http://ex/a> <http://ex/p> <http://ex/b> .")["conforms"]')
git diff --exit-code
