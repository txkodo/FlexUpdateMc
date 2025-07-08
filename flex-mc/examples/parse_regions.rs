use anyhow::Result;
use flex_mc::infra::region_loader::Dimension;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let dim = Dimension::new(PathBuf::from("examples/resource/regions"));
    let mut reg = dim.load_region((0, 0))?;
    let chunk = reg.load_chunk((0, 0))?;
    if let Some(chunk) = chunk {
        // Example: Print the block states of the first block in the chunk
        if let Ok(block) = chunk.get_block(0, 0, 0) {
            println!("Block state at (0, 0, 0): {:?}", block);
        } else {
            println!("No block state found at (0, 0, 0)");
        }
    } else {
        println!("No chunk found at (0, 0)");
    }
    Ok(())
}
