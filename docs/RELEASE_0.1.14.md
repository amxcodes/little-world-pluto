# Pluto v0.1.14

This release publishes the local improvements developed since v0.1.6.

- Adds opt-in Hyper Glass alongside Standard and Liquid Glass, with cached
  optics, spring-driven interaction, reduced-motion support and responsive
  control layouts.
- Refines profile dropdowns/editor tabs, pinning, dictation states and Settings
  text weight across all three appearance modes and both themes.
- Improves Meetings, Notes/reminder calendar, session turns and audio device
  menus. Profiles uses one outer panel; session removal has a red-icon
  confirmation; Hyper Glass shows Shelf as an icon/count.
- Includes the existing ACP session separation, provider installation/update
  improvements, saved visual replay and general 3D construction/animation work
  from the local builds. 3D Visualize remains an explicit Agents-only toggle for
  eligible models, including supported DeepSeek and other OpenCode families.
- Removes the optional Sherpa wake runtime and model downloads. Wake detection
  uses Windows speech recognition.
- Keeps generated logs, captures, signing material, extracted references and
  historical work reports out of the source release. Required test fixtures and
  product documentation remain tracked.

The UI corrections do not change the 3D authoring contracts or renderer behavior.
Release preparation applies Rust formatting and equivalent lint simplifications.

Windows x64 installer and portable ZIP are signed with the existing Pluto
release key. Keep app data when upgrading. Extract the portable ZIP before
running its executable. The release assets include SHA-256 checksums, SBOMs and
the build manifest.
