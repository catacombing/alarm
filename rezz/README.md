# Rezz — DBus RTC alarm daemon

Rezz is a DBus server which manages alarm clocks and automatically sets the
system RTC alarm to wakeup when the deadline for an alarm is reached.

The alarms managed by Rezz are persisted on the filesystem and will be reloaded
after reboot.

## Installation

Besides compiling Rezz using `cargo build`, it is necessary to add the [DBus
config](./org.catacombing.rezz.conf) to `/usr/share/dbus-1/system.d/`.

To manage Rezz with systemd, you might also want to install the [service
file](./rezz.service).
