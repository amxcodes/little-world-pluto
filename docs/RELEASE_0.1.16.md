# Pluto v0.1.16

- Embeds Pluto's Windows icon in the application executable so Explorer, the
  Start Menu shortcut and the notification-area icon use the product artwork.
- Reports native shell startup panics with their message and the panic log path
  instead of the generic “thread stopped” error.
- Keeps the existing Windows speech-recognition wake-word engine and current
  application behavior.

The v0.1.6 dialog means its native shell thread ended before reporting
readiness. That build has no panic logger, and no local panic log was present,
so the screenshot cannot identify the underlying machine-specific trigger.
Install v0.1.16 to get the branded icon and a specific panic message plus log
path if the startup failure recurs.
