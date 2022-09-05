use super::ConnectionBuilder;

pub struct Tacd {}

#[cfg(not(feature = "stub_out_dbus"))]
#[zbus::dbus_interface(name = "de.pengutronix.tacd1")]
impl Tacd {
    fn get_version(&mut self) -> String {
        std::env!("VERSION_STRING").to_string()
    }
}

impl Tacd {
    pub fn new() -> Self {
        Self {}
    }

    pub fn serve(self, cb: ConnectionBuilder) -> ConnectionBuilder {
        cb.serve_at("/de/pengutronix/tacd", self).unwrap()
    }
}
