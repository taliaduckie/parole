# Toward a Praat replacement

Start with a phonetics workflow we can actually compare: open a recording
and its TextGrid, segment and label it, verify an analysis, then hand the
files to someone still using Praat. Compatibility is a release criterion;
UI improvements should make that same work faster.

| Milestone | What to build and how to verify it |
| --- | --- |
| 1. TextGrid interchange | Open Praat long/short text, show interval and point tiers against audio, edit labels, and export a non-overwriting copy. Round-trip Praat-generated files and check timing, tier names/order, and labels. Binary TextGrid is outside this slice. |
| 2. Segmentation editor | Add, move and remove boundaries and points; add and rename tiers; undo/redo every edit. Keep the annotation cursor, playback and views synchronized. Test keyboard-only annotation and crash recovery before suggesting real work. |
| 3. Comparable measurements | Benchmark against Praat settings and fixtures for pitch, formants, intensity and spectrograms. Include silence, noise and ambiguous voicing, make parameter choices visible, and export measurements with settings. |
| 4. Repeatable workflows and releases | Add a documented batch interface for TextGrid and measurement operations, then a narrow scripting API. Lua will not run existing Praat scripts; track compatibility needs separately. Build a packaged macOS release and test file dialogs, playback and mic permission on a real Mac. |

What could make Parole better: a keyboard-first annotator that keeps audio,
spectrogram and labels aligned; instant previews for parameter changes;
search and jump across tiers; and analysis settings that travel with an
export. Compare timed tasks and correction rates against Praat on the same
recordings instead of claiming a general UX or accuracy advantage.
