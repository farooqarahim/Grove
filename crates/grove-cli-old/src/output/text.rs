use anyhow::Result;

pub fn print(body: &str) -> Result<()> {
    println!("{body}");
    Ok(())
}
