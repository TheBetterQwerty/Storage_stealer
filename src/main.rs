use std::io::{self, Write};
use std::collections::HashMap;
use std::env;

use crate::sender::FileStruct;

mod crypto;
mod argparse;
mod sender;

#[tokio::main]
async fn main() {
    let token = match env::var("GITHUB_TOKEN") {
        Ok(x) => x,
        Err(_) => {
            eprintln!("[!] Error: GITHUB_TOKEN variable not set");
            return;
        }
    };

    let username = match env::var("GITHUB_USERNAME") {
        Ok(x) => x,
        Err(_) => {
            eprintln!("[!] Error: GITHUB_USERNAME variable not set");
            return;
        }
    };

    let mut github_obj = sender::Github::new(&token, &username);
    github_obj.get_repos().await; // Put Repos in Cache

    match argparse::argparser() {
        argparse::Parser::List => {
            let files = list_files(&github_obj).await;
            if files.is_empty() {
                println!("[!] Cloud Seems Empty!");
                return;
            }

            println!("[*] Files in Cloud: ");
            for (index, (file_name, _)) in files.iter().enumerate() {
                println!("{}.   {}", index + 1, file_name);
            }
        },

        argparse::Parser::Upload(path) => {
            if let Ok(false) = std::fs::exists(&path) {
                eprintln!("[!] Error: File doesn't exists");
                return;
            }

            github_obj.upload_file(&path).await;
        },

        argparse::Parser::Download(file, path) => {
            let files = list_files(&github_obj).await;
            let chunks = files.get(&file);

            if chunks.is_none() {
                eprintln!("[!] File doesn't exists in cloud!");
                return;
            }

            if path.is_none() {
                eprintln!("[!] Please specify a output file");
                return;
            }

            github_obj.download_file(chunks.unwrap(), &path.unwrap()).await;
        },

        argparse::Parser::Delete(file, confirm) => {
            let files = list_files(&github_obj).await;
            let chunks = files.get(&file);

            if chunks.is_none() {
                eprintln!("[!] File not found in cloud");
                return;
            }

            if confirm.is_none() {
                let choice = input("[#] Do you wish to delete this file from the cloud ? (Y/n) ");
                if let Some('n') = choice.to_lowercase().chars().nth(0) {
                    println!("[#] File not deleted");
                    return;
                }
            }

            github_obj.delete_file(chunks.unwrap()).await;
        },
        _ => {}
    }
}

async fn list_files(github_obj: &sender::Github) -> HashMap<String, Vec<FileStruct>> {
    let mut files = HashMap::new();

    for repo in &github_obj.repos {
        files.extend(github_obj.files_in_repo(&repo.name, None).await);
    }

    files
}

fn input(data: &str) -> String {
    let mut input = String::new();
    print!("{}", data);
    let _ = std::io::stdout().flush();
    io::stdin().read_line(&mut input).expect("Error: Reading from stdin!");
    input.trim_end().to_string()
}
