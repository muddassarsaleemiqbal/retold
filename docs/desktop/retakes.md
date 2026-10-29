# Targeted retakes

Scenes offers **Redo this frame** beside a recorded frame and Narration offers **Record
narration again**. Each first shows a read-only review naming the stages that rebuild; only
confirmation changes anything. A frame retake advances exactly that frame's `image-<unit>-0`
revision, so the next reviewed run redraws only that frame (using its saved QA findings as the
correction when it failed QA) and rebuilds visual descendants. A narration retake advances the
voice revision. No job is queued and no allowance is created: continuing still needs a separately
reviewed run.

Reviews bind library, video identity, the target scope and its revision. Confirming the same
review twice is a no-op (lost-response retry); a review older than the current revision, an
unknown scene or a mismatched target is rejected. Two engine regressions in `tests/retake.rs`
cover these rules and prove that other frames' revisions and storyboard completion are untouched.
Not yet covered: retakes for character sheets, voice of a single edition, and native journeys.
