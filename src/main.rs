use std::env::args;

mod github;
mod fuse;
mod sender;

fn handle_download() {}

fn main() {
    let args: Vec<String> = args().collect();
    if args.len() < 3 {
        handle_download();
        return;
    }

    if args[1].eq("--upload") {
        let file_name = &args[2];
        // upload the file!
        return;
    }

    println!("[?] Usage: {} --upload <file>", args[0]);
}
