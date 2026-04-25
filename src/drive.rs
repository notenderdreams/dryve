use anyhow::Result;

pub fn fetch_drive(url: &str) -> Result<String> {
    let body = reqwest::blocking::get(url)?.text()?;
    Ok(body)
}