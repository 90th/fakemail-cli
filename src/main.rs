mod client;
mod commands;
mod html;
mod repl;

use client::FakeMailClient;
use commands::{GREEN, RED, RESET, print_inbox_json, print_mailbox, print_status};
use html::parse_duration;
use std::env;

fn print_help() {
    println!("FakeMail.net Command Line Interface");
    println!();
    println!("Usage:");
    println!("  fakemail-cli [options]");
    println!();
    println!("Options:");
    println!("  --status          Print current active email and exit");
    println!("  --new             Generate a new random email address and exit");
    println!("  --custom <name>   Set a custom email prefix and exit");
    println!("  --extend <time>   Extend mailbox lifetime (e.g. 10m, 1d, 3d, 5d, 1w, 2w) and exit");
    println!("  --list            List emails in the inbox and exit");
    println!("  --read <id>       Read a specific email content by ID and exit");
    println!("  --delete <id>     Delete a specific email by ID and exit");
    println!("  --help, -h        Show this help message");
}

fn init_client() -> FakeMailClient {
    match FakeMailClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{RED}Error initializing client: {e}{RESET}");
            std::process::exit(1);
        }
    }
}

fn require_arg<'a>(args: &'a [String], opt: &str, label: &str) -> &'a str {
    if args.len() < 3 {
        eprintln!("{RED}Error: {opt} requires {label}.{RESET}");
        std::process::exit(1);
    }
    &args[2]
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() == 1 {
        let client = init_client();
        repl::run_interactive(client);
        return;
    }

    let opt = args[1].as_str();
    match opt {
        "--help" | "-h" => {
            print_help();
        }
        "--status" => {
            let mut client = init_client();
            print_status(&mut client, "Password:");
        }
        "--new" => {
            let mut client = init_client();
            if let Err(e) = client.init_new_session() {
                eprintln!("{RED}Error: {e}{RESET}");
                std::process::exit(1);
            }
            print_mailbox(&client, "Active Email:", "Password:");
        }
        "--custom" => {
            let prefix = require_arg(&args, "--custom", "a username");
            let mut client = init_client();
            match client.set_custom_username(prefix) {
                Ok(email) => println!("{}", email),
                Err(e) => {
                    eprintln!("{RED}Error: {e}{RESET}");
                    std::process::exit(1);
                }
            }
        }
        "--extend" => {
            let duration = require_arg(&args, "--extend", "a duration");
            let mut client = init_client();
            match parse_duration(duration) {
                Ok(secs) => {
                    if let Err(e) = client.extend_lifetime(secs) {
                        eprintln!("{RED}Error: {e}{RESET}");
                        std::process::exit(1);
                    }
                    println!("{GREEN}Mailbox lifetime extended by {secs} seconds.{RESET}");
                }
                Err(e) => {
                    eprintln!("{RED}Error: {e}{RESET}");
                    std::process::exit(1);
                }
            }
        }
        "--list" => {
            let mut client = init_client();
            match client.get_inbox() {
                Ok(emails) => {
                    if let Err(e) = print_inbox_json(&emails) {
                        eprintln!("{RED}Error: {e}{RESET}");
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("{RED}Error: {e}{RESET}");
                    std::process::exit(1);
                }
            }
        }
        "--read" => {
            let email_id = require_arg(&args, "--read", "an email ID");
            let mut client = init_client();
            match client.read_email(email_id) {
                Ok(body) => println!("{}", body),
                Err(e) => {
                    eprintln!("{RED}Error: {e}{RESET}");
                    std::process::exit(1);
                }
            }
        }
        "--delete" => {
            let email_id = require_arg(&args, "--delete", "an email ID");
            let mut client = init_client();
            match client.delete_email(email_id) {
                Ok(_) => println!("{GREEN}Deleted email ID {email_id}{RESET}"),
                Err(e) => {
                    eprintln!("{RED}Error: {e}{RESET}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("{RED}Unknown option: {opt}{RESET}");
            print_help();
            std::process::exit(1);
        }
    }
}
