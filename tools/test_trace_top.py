import unittest

from trace_top import ready_at, totals


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

    def test_only_after_ready_unless_asked(self):
        events = [
            {"ph": "B", "ts": 0, "tid": 1, "name": "boot"},
            {"ph": "E", "ts": 10, "tid": 1, "name": "boot"},
            {"ph": "B", "ts": 10, "tid": 1, "name": "ready"},
            {"ph": "E", "ts": 10, "tid": 1, "name": "ready"},
            {"ph": "B", "ts": 12, "tid": 1, "name": "frame"},
            {"ph": "E", "ts": 15, "tid": 1, "name": "frame"},
        ]
        since = ready_at(events)
        self.assertEqual(since, 10)
        self.assertNotIn("boot", totals(events, since)[0])
        self.assertEqual(totals(events, since)[0]["frame"], 3)
        self.assertEqual(totals(events)[0]["boot"], 10)
        self.assertIsNone(ready_at(events[:2]))


if __name__ == "__main__":
    unittest.main()
