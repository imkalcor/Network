use std::{
    net::{SocketAddr, UdpSocket},
    ops::Deref,
    sync::Arc,
    time::Instant,
};

pub struct RakStream {
    addr: SocketAddr,
    socket: Arc<UdpSocket>,
    activity: Instant,
}

impl RakStream {
    pub fn new(addr: SocketAddr, socket: Arc<UdpSocket>) -> Self {
        Self {
            addr,
            socket,
            activity: Instant::now(),
        }
    }
}

impl Deref for RakStream {
    type Target = UdpSocket;

    fn deref(&self) -> &Self::Target {
        &self.socket
    }
}
