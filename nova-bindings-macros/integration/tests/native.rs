//! The annotated Rust items keep their original API.

use nova_bindings_integration::ExpMovingAverage;
use nova_bindings_integration::counter::Counter;
use nova_ta_core::Indicator;

#[test]
fn rust_api_is_unchanged() {
    let mut ema = ExpMovingAverage::<f64>::new(3, 2.0).unwrap();
    // `update` still resolves to the trait method, not the Python adapter.
    assert_eq!(ema.update(1.0).unwrap(), 1.0);
    assert_eq!(ema.update_batch(vec![2.0, 3.0]).unwrap(), vec![1.5, 2.25]);
    assert_eq!(ema.seen(), 3);

    let mut counter = Counter::above(0.5);
    assert_eq!(counter.update(1.0).unwrap(), 1.0);
    assert_eq!(counter.threshold(), 0.5);
}
