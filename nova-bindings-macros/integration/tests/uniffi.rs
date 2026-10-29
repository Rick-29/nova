//! Calls the generated UniFFI objects from Rust, as the scaffolding does.
#![cfg(feature = "uniffi")]

use std::sync::Arc;
use std::thread;

use nova_bindings_integration::FixtureError;
use nova_bindings_integration::UniffiExpMovingAverage;
use nova_bindings_integration::counter::UniffiCounter;

#[test]
fn constructs_and_updates_with_uniffi_types() {
    // `usize` crosses UniFFI as `u64`.
    let ema = UniffiExpMovingAverage::new(3u64, 2.0).unwrap();
    assert_eq!(ema.update(1.0).unwrap(), 1.0);
    assert_eq!(ema.update(2.0).unwrap(), 1.5);
    assert!(!ema.ready());
    assert_eq!(ema.update(3.0).unwrap(), 2.25);
    assert!(ema.ready());
    assert_eq!(ema.samples(), 3u64);
    assert_eq!(ema.last(), Some(2.25));
    ema.reset();
    assert!(!ema.ready());
    assert_eq!(ema.last(), None);
}

#[test]
fn errors_use_the_configured_type() {
    assert_eq!(
        UniffiExpMovingAverage::new(0, 2.0).err(),
        Some(FixtureError::InvalidPeriod)
    );
    let ema = UniffiExpMovingAverage::new(2, 2.0).unwrap();
    assert!(matches!(
        ema.update(f64::INFINITY),
        Err(FixtureError::NonFinite(_))
    ));
}

#[test]
fn concurrent_updates_are_serialized() {
    let counter: Arc<UniffiCounter> = UniffiCounter::new(0.0);
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let counter = Arc::clone(&counter);
            thread::spawn(move || {
                for _ in 0..1000 {
                    counter.update(1.0).unwrap();
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(counter.update(0.0).unwrap(), 8000.0);
    assert_eq!(counter.threshold(), 0.0);
}
