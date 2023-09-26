//! Rezz DBus interface.

use rezz::Alarm;
use zbus::dbus_proxy;

#[dbus_proxy(
    interface = "org.catacombing.rezz",
    default_service = "org.catacombing.rezz",
    default_path = "/org/catacombing/rezz"
)]
trait Rezz {
    fn add_alarm(&self, alarm: Alarm) -> zbus::Result<()>;

    fn remove_alarm(&self, id: String) -> zbus::Result<()>;

    #[dbus_proxy(property)]
    fn alarms(&self) -> zbus::Result<Vec<Alarm>>;
}
