use std::env::args;

mod sender;

fn handle_download() {}

fn main() {
    // Main Function calls get_repos().await on startup

    let args: Vec<String> = args().collect();
    if args.len() < 3 {
        handle_download();
        return;
    }

    if !args[1].eq("--upload") {
        println!("[?] Usage: {} --upload <file>", args[0]);
        return;
    }

    let file_name = String::from(&args[2]);
    // Read token from .env file and username too
    let github_obj = sender::Github::new();
}
