use std::env::args;

#[derive(PartialEq)]
pub enum Parser {
    List,
    Invalid(String),
    Download(String, Option<String>), // Input, Output
    Upload(String), // File
    Delete(String, Option<String>), // File, --yes OR None
    Exit, // main just returns
}

pub fn argparser() -> Parser {
    let mut args = args();
    let prog_name = args.next().unwrap_or_else(|| "ghfs".to_string());

    let missing_cmd = |x: &str| {
        println!("[!] Missing arguments for '{}'. Try {} --help", x, prog_name);
    };

    let mut input: Option<String> = None;
    let mut output: Option<String> = None;

    while let Some(cmd) = args.next() {
        match cmd.as_str() {
            "--version" | "-v" => {
                print_version();
                return Parser::Exit;
            }

            "--list" => return Parser::List,

            "-h" | "--help" => {
                print_help(&prog_name);
                return Parser::Exit;
            }

            "--upload" => {
                if let Some(file) = args.next() {
                    return Parser::Upload(file);
                } else {
                    missing_cmd("--upload");
                    return Parser::Invalid("missing file".into());
                }
            }

            "--delete" => {
                if let Some(file) = args.next() {
                    let flag = match args.next() {
                        Some(x) if x == "--yes" => Some(x),
                        Some(x) => return Parser::Invalid(format!("unknown flag '{}'", x)),
                        None => None,
                    };
                    return Parser::Delete(file, flag);
                } else {
                    missing_cmd("--delete");
                    return Parser::Invalid("missing file".into());
                }
            }

            "--download" => {
                input = args.next();
                if input.is_none() {
                    missing_cmd("--download");
                    return Parser::Invalid("missing input".into());
                }
            }

            "--output" | "-o" => {
                output = args.next();
                if output.is_none() {
                    missing_cmd("--output");
                    return Parser::Invalid("missing output".into());
                }
            }

            _ => return Parser::Invalid(cmd),
        }
    }

    if let Some(input) = input {
        return Parser::Download(input, output);
    }

    print_help(&prog_name);
    Parser::Exit
}

fn print_help(prog_name: &str) {
    println!("Usage:");
    println!("  {} <command> [options]", prog_name);

    println!("\nCommands:");
    println!("  --version                           Displays current version");
    println!("  --list                              List all files saved");
    println!("  --upload <path>                     Upload file to github");
    println!("  --download <file> -o <output>       Download file from github locally");
    println!("  --delete <file>                     Delete file from github");
}

fn print_version() {
    println!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
}
