## 1. Theme

- [x] 1.1 Bundle Chakra Petch (Regular, SemiBold) and Press Start 2P with their OFL licences; register them at startup
- [x] 1.2 `theme.rs`: chrome colours, class colours per launcher, alpha helper, with tests
- [x] 1.3 "Arcade night" terminal palette (update palette/session tests)
- [x] 1.4 `[ui] scanlines` config key (default on) and scanline painting, with tests

## 2. Chrome

- [x] 2.1 HUD bar: pixel logo, session counter, version, neon gradient line
- [x] 2.2 Sidebar: workspace headers, tinted launcher buttons with hover glow, session lights and active gradient
- [x] 2.3 Tab bar: status light, glowing accent on the active tab, close button
- [x] 2.4 Glowing frame in the class colour around the active terminal; empty state

## 3. Verification

- [x] 3.1 Screenshots on Windows: chrome readable, agent launcher runs `claude` in the project, Ctrl+Shift+T leaves no stray character
- [x] 3.2 CI green on Windows, macOS and Linux
