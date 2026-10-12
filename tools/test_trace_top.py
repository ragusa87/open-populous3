import unittest

from trace_top import totals


class TotalsTest(unittest.TestCase):
    def test_pairs_per_thread_and_complete_events(self):
        events = [
            {"ph": "B", "ts": 0, "tid": 1, "name": "system", "args": {"name": "a"}},
            {"ph": "B", "ts": 1, "tid": 2, "name": "system", "args": {"name": "a"}},
            {"ph": "E", "ts": 5, "tid": 1, "name": "system", "args": {"name": "a"}},
            {"ph": "E", "ts": 4, "tid": 2, "name": "system", "args": {"name": "a"}},
            {"ph": "X", "ts": 0, "dur": 7, "tid": 1, "name": "frame"},
        ]
        total, calls = totals(events)
        self.assertEqual((total["system: a"], calls["system: a"]), (8, 2))
        self.assertEqual(total["frame"], 7)


if __name__ == "__main__":
    unittest.main()
