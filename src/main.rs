use std::env::args;
mod github;

const USER_AGENT: &str = "Mozilla/5.0 (Linux; Android 16; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.12.45 Mobile Safari/537.36";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    let argv = args().collect::<Vec<String>>();

    let mut github = github::Github::new(&argv[1], USER_AGENT, &argv[2]);

    // mount
    github.cache_files().await?;

    /* works
    if github.upload_file(&argv[3]).await {
        println!("File Uploaded sucessfully");
    }


    let data = github.download_file(&argv[3]).await; // tmp
    if data.is_empty() {
        println!("Empty");
        return Ok(());
    }

    let _ = std::fs::write("/tmp/lol", data);
    println!("Written to /tmp/lol"); */


    dbg!(github.update_file_content(
        "tmp",
        "hello fucking losers"
    ).await);

    Ok(())
}
