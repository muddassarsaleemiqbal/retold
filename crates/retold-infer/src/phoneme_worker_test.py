"""Offline acoustic timing regressions; run in the pinned worker environment."""
import unittest
import numpy as np
from phoneme_worker import decode


class AlignmentTests(unittest.TestCase):
    labels = {0: "<pad>", 4: "m", 5: "a"}

    def test_repeated_phones_separated_by_blank_keep_both_runs(self):
        p = np.full((3, 6), .001)
        p[0, 4] = p[1, 0] = p[2, 4] = .99
        result = decode(np.log(p), self.labels, 0, [(0, .1)])
        self.assertEqual([p["ipa"] for p in result], ["m", "m"])
        self.assertAlmostEqual(result[0]["end"], result[1]["start"])

    def test_measured_phone_boundaries_and_nonzero_clock(self):
        p = np.full((6, 6), .001)
        p[:3, 4] = .99
        p[3:, 5] = .99
        result = decode(np.log(p), self.labels, 10, [(10, 10.2)])
        self.assertEqual([p["ipa"] for p in result], ["m", "a"])
        self.assertAlmostEqual(result[0]["end"], 10.0625)
        self.assertAlmostEqual(result[1]["start"], 10.0625)

    def test_silence_and_outside_speech_do_not_create_phones(self):
        p = np.full((3, 6), .001)
        p[:, 0] = .99
        with self.assertRaises(ValueError):
            decode(np.log(p), self.labels, 0, [(0, 1)])
        p[:, 4] = 1
        with self.assertRaises(ValueError):
            decode(np.log(p), self.labels, 0, [(1, 2)])

    def test_low_confidence_is_rejected(self):
        p = np.full((3, 6), .1)
        p[:, 4] = .15
        with self.assertRaises(ValueError):
            decode(np.log(p), self.labels, 0, [(0, 1)])


if __name__ == "__main__":
    unittest.main()
