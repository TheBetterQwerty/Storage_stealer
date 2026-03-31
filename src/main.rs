use std::{env::args, io::{self, Read, Write}};
use std::collections::HashMap;

mod sender;

#[tokio::main]
async fn main() {
    let args: Vec<String> = args().collect();

    // Read token from .env file and username too
    let (token , username) = {
        let mut buffer = String::new();

        let _ = std::fs::File::open(".env")
            .expect("Error: Reading from file")
            .read_to_string(&mut buffer);

        let z: Vec<String> = buffer.split(' ').map(|x| x.into()).collect();

        (z[0].clone(), z[1].clone())
    };

    let mut github_obj = sender::Github::new(&token, &username);
    github_obj.get_repos().await; // Put Repos in Cache

    if args.len() < 3 {
        handle_download(github_obj).await;
        return;
    }

    if !args[1].eq("--upload") {
        println!("[?] Usage: {} --upload <file>", args[0]);
        return;
    }

    let file_name = &args[2];

    github_obj.upload_file(file_name).await;
}

async fn handle_download(mut github_obj: sender::Github) {
    // Print all the files
    let mut files = HashMap::new();

    for repo in &github_obj.repos {
        files.extend(github_obj.files_in_repo(&repo.name, None).await);
    }

    println!("[#] Files in Repo's: ");
    for (repo, _) in files.iter() {
        println!("{}", repo);
    }

    let choice = match input("Do u want to download or delete ? (1/2)").trim().parse::<u8>() {
        Ok(x) => x,
        Err(err) => {
            println!("[!] Error: {err}");
            return;
        }
    };

    let choose = input("Enter file name: ");

    if files.get(&choose).is_none() {
        println!("[!] Error: {} doesn't exists!", choose);
        return;
    }

    match choice {
        1 => {
            // github_obj.download_file(required_file, output_file)
        },
        2 => {
            // delete the selected file
            github_obj.delete_file(&choose).await;
            println!("[#] {} file was deleted", choose);
        },
        _ => {
            println!("[!] Error: Invalid Option!");
            return;
        }
    }
}

fn input(data: &str) -> String {
    let mut input = String::new();
    print!("{}", data);
    let _ = std::io::stdout().flush();
    io::stdin().read_line(&mut input).expect("Error: Reading from stdin!");
    input
}
