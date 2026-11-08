# Appearance and interaction

Pluto has three independent appearance modes in Settings → General:
Standard, Liquid Glass and Hyper Glass. Hyper Glass is opt-in; choosing it does
not replace the other modes or change the 3D authoring pipeline.

The native Windows shell owns the island outline, height and solid backing.
The WebView paints its content and controls without a rectangular root backdrop.
Reading panes use quiet surfaces; actions use material-specific fills and rims.
Light navigation uses an inset rim so its horizontally scrolling viewport does
not cut off a shadow.

Hyper Glass uses delegated input handling and a shared spring scheduler for
selection lenses, presses and toggle thumbs. Optical fields are cached, prepared
on demand and disabled during scrolling. The scheduler stops at rest, on focus
loss and while the page is hidden. Dense menu/list/date controls use small press
motion instead of expanding into adjacent rows. Reduced-motion preferences keep
the same state and keyboard behavior with motion disabled.

Shared secondary controls are styled in `island-ui/src/control-polish.css`.
Profile menus, pinning and dictation keep their foreground and material layers
through hover. Recording retains its recording color. Settings selection uses
the control surface and icon rather than a heavier text weight. Profiles uses
one outer panel; the roster and editor are layout sections within it.

The reminder calendar preserves six weeks of date cells, stacks the time panel
when its available width is small, and scrolls in short windows. A selected date,
today indicator and keyboard focus remain distinct. Session deletion arms a red
icon before the second click sends removal. The Hyper Glass Shelf dock shows the
icon and count while retaining an accessible Shelf label.

## Validation

Run `node --test tests/*.test.mjs` in `island-ui` for source-level regressions.
Optional browser suites use an existing Playwright installation through
`PLUTO_PLAYWRIGHT_PATH` and a Chromium executable through `PLUTO_BROWSER_PATH`.
`control-polish-browser.mjs` covers profile menus/editor tabs, pinning, dictation
states and Settings typography in all three modes, both themes and two widths.
The glass and Hyper Glass suites cover navigation, layout, pressed geometry,
toggle behavior and cleanup. Their IPC, providers and audio input are mocked.
Generated captures/reports belong under the ignored `target/ui-review/` folder.

Historical development reports and unused session captures are kept locally in
the ignored `local-notes/` archive. Required regression fixtures, product docs,
licenses and release notes remain in the repository.
