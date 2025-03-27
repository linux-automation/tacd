//! This code was generated by `zbus-xmlgen` `4.1.0` from DBus introspection data.
//!
//! By running `zbus-xmlgen system de.pengutronix.rauc /` on the LXA TAC.

use zbus::proxy;

#[proxy(
    interface = "de.pengutronix.rauc.Poller",
    default_service = "de.pengutronix.rauc",
    default_path = "/"
)]
trait Poller {
    /// Poll method
    fn poll(&self) -> zbus::Result<()>;

    /// NextPoll property
    #[zbus(property)]
    fn next_poll(&self) -> zbus::Result<i64>;

    /// Status property
    #[zbus(property)]
    fn status(&self)
        -> zbus::Result<std::collections::HashMap<String, zbus::zvariant::OwnedValue>>;
}
