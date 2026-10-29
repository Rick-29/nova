// Runs against this crate built as a Node addon:
//   cargo build -p nova-bindings-integration --features napi
//   copy the cdylib to <file>.node, then NOVA_NAPI_ADDON=<file>.node node --test js/napi.test.cjs
const test = require("node:test");
const assert = require("node:assert/strict");

const addon = require(process.env.NOVA_NAPI_ADDON);

test("only selected backends are exported", () => {
  // `Counter` selects python, uniffi and wasm only.
  assert.deepEqual(Object.keys(addon), ["Ema"]);
});

test("constructor, update, ready and reset delegate to the indicator", () => {
  const ema = new addon.Ema(3, 2.0);
  assert.equal(ema.update(1.0), 1.0);
  assert.equal(ema.update(2.0), 1.5);
  assert.equal(ema.ready(), false);
  assert.equal(ema.update(3.0), 2.25);
  assert.equal(ema.ready(), true);
  // `usize` is returned as a JavaScript number.
  assert.equal(ema.samples(), 3);
  ema.reset();
  assert.equal(ema.ready(), false);
  assert.equal(ema.last(), null);
});

test("errors are thrown with the Rust message", () => {
  assert.throws(() => new addon.Ema(0, 2.0), { message: "period must be at least 1" });
  const ema = new addon.Ema(2, 2.0);
  assert.throws(() => ema.update(Infinity), /input must be finite/);
});
