use std::path::PathBuf;

use crate::model::McDimension;

pub trait WorldHandler {
    fn get_level_dat(&self) -> Result<PathBuf, String>;
    fn get_region_dir(&self, dimension: &McDimension) -> Result<PathBuf, String>;
}

pub struct WorldHandlerImpl {
    pub path: PathBuf,
}

impl WorldHandler for WorldHandlerImpl {
    fn get_level_dat(&self) -> Result<PathBuf, String> {
        let level_dat = self.path.join("level.dat");
        if level_dat.exists() {
            Ok(level_dat)
        } else {
            Err("level.dat not found".to_string())
        }
    }

    fn get_region_dir(&self, dimension: &McDimension) -> Result<PathBuf, String> {
        let mca_dir = match dimension {
            McDimension::Overworld => self.path.join("world").join("region"),
            McDimension::Nether => {
                let plugin_dir = self.path.join("world_nether").join("DIM-1");
                if plugin_dir.exists() {
                    plugin_dir.join("region")
                } else {
                    self.path.join("world").join("DIM-1").join("region")
                }
            }
            McDimension::TheEnd => {
                let plugin_dir = self.path.join("world_the_end").join("DIM1");
                if plugin_dir.exists() {
                    plugin_dir.join("region")
                } else {
                    self.path.join("world").join("DIM1").join("region")
                }
            }
            _ => return Err(format!("Unsupported dimension: {:?}", dimension)),
        };
        return Ok(mca_dir);
    }
}
