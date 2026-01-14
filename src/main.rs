use std::env::args;

use fuser::MountOption;

mod github;
mod fuse;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    let argv = args().collect::<Vec<String>>();

    if argv.len() < 4 {
        println!("[?] Usage: {} <mount-point> <token> <username>", argv[0]);
        return Ok(());
    }

    let mut github = github::Github::new(&argv[2], &argv[3]);
    github.cache_files().await?;

    let files: Vec<_> = github.cache
            .expect("Error: getting values")
            .values()
            .flat_map(|s| s.clone())
            .collect();

    let fs = github::FileTree::new(files);

    fuser::mount2(fs, &argv[1], &[
        MountOption::RO,
        MountOption::FSName("Github_FS".to_string()),
        MountOption::DefaultPermissions,
    ])?;

    // sync the cache (if files not sync then sync it)

    Ok(())
}
