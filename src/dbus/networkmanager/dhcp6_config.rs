//! This code was generated by `zbus-xmlgen` `4.1.0` from DBus introspection data.
//!
//! By manually running
//!
//! zbus-xmlgen system org.freedesktop.NetworkManager /org/freedesktop/NetworkManager/DHCP6Config/<ID>
//!
//! For all <ID>s on the LXA TAC and manually combining the results.

use zbus::proxy;

#[proxy(
    interface = "org.freedesktop.NetworkManager.DHCP6Config",
    default_service = "org.freedesktop.NetworkManager"
)]
trait DHCP6Config {
    /// Options property
    #[zbus(property)]
    fn options(
        &self,
    ) -> zbus::Result<std::collections::HashMap<String, zbus::zvariant::OwnedValue>>;
}
