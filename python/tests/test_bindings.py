"""Tests for the generated `nova_ta` Python bindings.

Build first: `maturin develop` (from the repository root), then run
`python -m unittest discover python/tests`.
"""

import inspect
import math
import unittest

import nova_ta
from nova_ta import Cti, NovaError, UltimateSmoother


def reference_smoother(period, values):
    f = math.sqrt(2) * math.pi / period
    a1 = math.exp(-f)
    c2 = 2 * a1 * math.cos(f)
    c3 = -a1 * a1
    c1 = (1 + c2 - c3) / 4
    out = []
    for i, x in enumerate(values):
        if i < 2:
            out.append(x)
        else:
            out.append(
                (1 - c1) * x
                + (2 * c1 - c2) * values[i - 1]
                - (c1 + c3) * values[i - 2]
                + c2 * out[i - 1]
                + c3 * out[i - 2]
            )
    return out


class CtiTests(unittest.TestCase):
    def test_trend_values(self):
        cti = Cti(5)
        for value in [1.0, 2.0, 3.0, 4.0]:
            cti.update(value)
        self.assertFalse(cti.ready())
        self.assertAlmostEqual(cti.update(5.0), 1.0)
        self.assertTrue(cti.ready())

    def test_matches_rust_results(self):
        cti = Cti(5)
        result = None
        for value in [1.0, 3.0, 2.0, 5.0, 4.0]:
            result = cti.update(value)
        self.assertAlmostEqual(result, 0.8, places=6)

    def test_reset_restarts_warmup(self):
        cti = Cti(3)
        for value in [1.0, 2.0, 3.0]:
            cti.update(value)
        self.assertTrue(cti.ready())
        self.assertIsNone(cti.reset())
        self.assertFalse(cti.ready())
        self.assertEqual(cti.update(100.0), 0.0)

    def test_invalid_period_raises_nova_error(self):
        with self.assertRaises(NovaError) as raised:
            Cti(1)
        self.assertIn("Invalid period", str(raised.exception))
        self.assertTrue(issubclass(NovaError, ValueError))

    def test_not_callable_by_default(self):
        self.assertFalse(hasattr(Cti(3), "__call__"))

    def test_metadata(self):
        self.assertEqual(Cti.__name__, "Cti")
        self.assertIn("Correlation Trend Indicator", Cti.__doc__)
        self.assertEqual(str(inspect.signature(Cti)), "(period)")
        self.assertIn("price observation", Cti.update.__doc__)
        self.assertEqual(str(inspect.signature(Cti.update)), "(self, /, input)")


class UltimateSmootherTests(unittest.TestCase):
    def test_matches_reference(self):
        values = [100 + math.sin(i * 0.7) * 5 + i * 0.1 for i in range(60)]
        for period in [2, 5, 10, 20]:
            smoother = UltimateSmoother(period)
            expected = reference_smoother(period, values)
            for value, want in zip(values, expected):
                self.assertAlmostEqual(smoother.update(value), want, places=9)

    def test_update_is_also_callable(self):
        method = UltimateSmoother(10)
        called = UltimateSmoother(10)
        for value in [7.0, 9.0, 8.0, 10.0, 11.0]:
            self.assertEqual(method.update(value), called(value))
        self.assertTrue(called.ready())

    def test_warmup_and_reset(self):
        smoother = UltimateSmoother(10)
        self.assertEqual(smoother.update(7.0), 7.0)
        self.assertEqual(smoother.update(9.0), 9.0)
        self.assertFalse(smoother.ready())
        smoother.update(8.0)
        self.assertTrue(smoother.ready())
        smoother.reset()
        self.assertFalse(smoother.ready())
        self.assertEqual(smoother.update(42.0), 42.0)

    def test_zero_period_raises(self):
        with self.assertRaises(NovaError):
            UltimateSmoother(0)

    def test_type_errors_come_from_pyo3(self):
        with self.assertRaises(TypeError):
            UltimateSmoother("ten")
        with self.assertRaises(OverflowError):
            UltimateSmoother(-1)


class ModuleTests(unittest.TestCase):
    def test_exports(self):
        for name in ["Cti", "UltimateSmoother", "NovaError"]:
            self.assertTrue(hasattr(nova_ta, name), name)


if __name__ == "__main__":
    unittest.main()
