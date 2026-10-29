"""Exercises the UniFFI-generated Python bindings of nova-ta.

Run `scripts/uniffi-python-test.sh`, which builds the library, generates
`nova_ta.py` with uniffi-bindgen into a temporary directory and runs this file
from there.
"""

import unittest

import nova_ta
from nova_ta import Cti, NovaError, UltimateSmoother


class UniffiBindingsTests(unittest.TestCase):
    def test_cti(self):
        cti = Cti(5)
        result = None
        for value in [1.0, 3.0, 2.0, 5.0, 4.0]:
            result = cti.update(value)
        self.assertAlmostEqual(result, 0.8, places=6)
        self.assertTrue(cti.ready())
        cti.reset()
        self.assertFalse(cti.ready())

    def test_ultimate_smoother_warmup(self):
        smoother = UltimateSmoother(10)
        self.assertEqual(smoother.update(7.0), 7.0)
        self.assertEqual(smoother.update(9.0), 9.0)
        self.assertFalse(smoother.ready())
        smoother.update(8.0)
        self.assertTrue(smoother.ready())

    def test_flat_error_variants(self):
        with self.assertRaises(NovaError.InvalidPeriod) as raised:
            Cti(1)
        self.assertIn("Invalid period", str(raised.exception))
        with self.assertRaises(NovaError):
            UltimateSmoother(0)

    def test_docs_are_exported(self):
        self.assertIn("Correlation Trend Indicator", Cti.__doc__)


if __name__ == "__main__":
    unittest.main()
