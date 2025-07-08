pub trait FreePortFinder {
    fn find_free_port(&self, host: std::net::IpAddr) -> Result<u16, std::io::Error>;
}

pub struct DefaultFreePortFinder;
impl FreePortFinder for DefaultFreePortFinder {
    fn find_free_port(&self, host: std::net::IpAddr) -> Result<u16, std::io::Error> {
        let listener = std::net::TcpListener::bind((host, 0));
        let listener = match listener {
            Ok(listener) => listener,
            Err(e) => return Err(e),
        };
        Ok(listener.local_addr()?.port())
    }
}
