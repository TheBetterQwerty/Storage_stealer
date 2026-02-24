use std::env::args;

use fuser::MountOption;
use tokio::runtime::Runtime;

mod github;
mod fuse;

fn main() {
    let rt = Runtime::new().expect("Error: handle creation error");

    let argv = args().collect::<Vec<String>>();

    if argv.len() < 4 {
        println!("[?] Usage: {} <mount-point> <token> <username>", argv[0]);
        return;
    }

    let mut github = github::Github::new(&argv[2], &argv[3]);
    if let Err(err) = std::fs::create_dir("/tmp/FS/") {
        eprintln!("Error: creating directory {err}");
        return;
    }

    rt.block_on(async {
        // run periodically after 10 minutes
        github.cache_files().await.unwrap();
    });

    let fs = github::FileTree::new(github, rt.handle().clone());

    println!("Before fuser");

    fuser::mount2(fs, &argv[1], &[
        MountOption::RW,
        MountOption::FSName("Github_FS".to_string()),
        MountOption::DefaultPermissions,
    ]).unwrap();

    println!("after fuser");
    // sync the cache (if files not sync then sync it)
    //

    if let Err(err) = std::fs::remove_dir_all("/tmp/FS/") {
        eprintln!("Error: deleting directory {err}");
        return;
    }
}
