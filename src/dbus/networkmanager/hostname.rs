use zbus::dbus_proxy;

#[dbus_proxy(
    default_service = "org.freedesktop.hostname1",
    interface = "org.freedesktop.hostname1",
    default_path = "/org/freedesktop/hostname1"
)]
trait Hostname {
    #[dbus_proxy(property)]
    fn hostname(&self) -> zbus::Result<String>;
}
