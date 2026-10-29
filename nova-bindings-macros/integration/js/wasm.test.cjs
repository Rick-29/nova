// Runs against the wasm-pack output of this crate:
//   wasm-pack build . --target nodejs --dev --out-dir <dir> --no-pack -- --features wasm
//   NOVA_WASM_PKG=<dir> node --test js/wasm.test.cjs
const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");

const pkg = require(path.join(process.env.NOVA_WASM_PKG, "nova_bindings_integration.js"));

test("constructor, update, ready and reset delegate to the indicator", () => {
  const ema = new pkg.Ema(3, 2.0);
  assert.equal(ema.update(1.0), 1.0);
  assert.equal(ema.update(2.0), 1.5);
  assert.equal(ema.ready(), false);
  assert.equal(ema.update(3.0), 2.25);
  assert.equal(ema.ready(), true);
  assert.equal(ema.samples(), 3);
  assert.equal(ema.last(), 2.25);
  ema.reset();
  assert.equal(ema.ready(), false);
  assert.equal(ema.last(), undefined);
});

test("errors are thrown with the Rust message", () => {
  assert.throws(() => new pkg.Ema(0, 2.0), { message: "period must be at least 1" });
  const ema = new pkg.Ema(2, 2.0);
  assert.throws(() => ema.update(Infinity), /input must be finite/);
});

test("explicit constructor and cross-module impl blocks", () => {
  const counter = new pkg.Counter(1.5);
  assert.equal(counter.threshold(), 1.5);
  assert.deepEqual([1, 2, 3].map((x) => counter.update(x)), [0, 1, 2]);
});
