use anyhow;
use async_std::sync::Arc;
use zbus::Connection;

mod manager;

#[derive(Clone)]
pub struct Systemd {
    conn: Arc<Connection>,
}

impl Systemd {
    pub fn new(conn: Arc<Connection>) -> Self {
        Self { conn }
    }

    pub async fn restart_service(&self, name: &str) -> anyhow::Result<()> {
        let manager = manager::ManagerProxy::new(&self.conn).await?;
        manager.restart_unit(name, "replace").await?;
        Ok(())
    }

    pub async fn reboot(&self) -> anyhow::Result<()> {
        let manager = manager::ManagerProxy::new(&self.conn).await?;
        manager.reboot().await?;
        Ok(())
    }
}
