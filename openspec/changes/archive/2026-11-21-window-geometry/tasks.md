## 1. Implementation

- [x] 1.1 `SavedWindow` with sanity checks, optional in `SavedState` (old files still load)
- [x] 1.2 Open with saved bounds when they are on a screen; maximized if it was
- [x] 1.3 Track bounds in memory; save with the sessions and when the window closes

## 2. Verification

- [x] 2.1 Unit tests (sanity, round trip, old files)
- [x] 2.2 Verify on Windows: opens with the saved size and position, saved again on close
