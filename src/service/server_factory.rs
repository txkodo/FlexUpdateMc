use crate::service::{server_handler::ServerHandler, world_handler::WorldHandler};
use std::path::Path;

pub enum ServerCreationMethod {
    WithRegion,
    WithoutRegion,
}

pub trait ServerFactory {
    fn create_server_handler_from_world(
        &self,
        world: &dyn WorldHandler,
        path: &Path,
        method: ServerCreationMethod,
    ) -> Result<Box<dyn ServerHandler>, String>;
}
