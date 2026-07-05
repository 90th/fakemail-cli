mod client;
mod html;
mod repl;

use client::FakeMailClient;
use html::{get_remaining_time, parse_duration};
use std::env;

const GREEN: &str = "\x1B[32m";
const RED: &str = "\x1B[31m";
const YELLOW: &str = "\x1B[33m";
const RESET: &str = "\x1B[0m";

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

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 {
        let mut client = match FakeMailClient::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{RED}Error initializing client: {e}{RESET}");
                std::process::exit(1);
            }
        };

        let opt = args[1].as_str();
        match opt {
            "--help" | "-h" => {
                print_help();
            }
            "--status" => {
                println!("Active Email:   {GREEN}{}{RESET}", client.email);
                if !client.password.is_empty() {
                    println!("Password:       {GREEN}{}{RESET}", client.password);
                }
                match client.get_lifetime() {
                    Ok((ted, konec)) => {
                        let rem = get_remaining_time(&ted, &konec);
                        println!("Remaining Time: {YELLOW}{}{RESET}", rem);
                    }
                    Err(_) => {
                        println!("Remaining Time: Unknown");
                    }
                }
            }
            "--new" => {
                if let Err(e) = client.init_new_session() {
                    eprintln!("{RED}Error: {e}{RESET}");
                    std::process::exit(1);
                }
                println!("Active Email:   {GREEN}{}{RESET}", client.email);
                if !client.password.is_empty() {
                    println!("Password:       {GREEN}{}{RESET}", client.password);
                }
            }
            "--custom" => {
                if args.len() < 3 {
                    eprintln!("{RED}Error: Custom prefix option requires a username.{RESET}");
                    std::process::exit(1);
                }
                let prefix = &args[2];
                match client.set_custom_username(prefix) {
                    Ok(email) => println!("{}", email),
                    Err(e) => {
                        eprintln!("{RED}Error: {e}{RESET}");
                        std::process::exit(1);
                    }
                }
            }
            "--extend" => {
                if args.len() < 3 {
                    eprintln!("{RED}Error: Extend option requires a duration.{RESET}");
                    std::process::exit(1);
                }
                let duration = &args[2];
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
            "--list" => match client.get_inbox() {
                Ok(emails) => {
                    println!("{}", serde_json::to_string_pretty(&emails).unwrap());
                }
                Err(e) => {
                    eprintln!("{RED}Error: {e}{RESET}");
                    std::process::exit(1);
                }
            },
            "--read" => {
                if args.len() < 3 {
                    eprintln!("{RED}Error: Read option requires an email ID.{RESET}");
                    std::process::exit(1);
                }
                let email_id = &args[2];
                match client.read_email(email_id) {
                    Ok(body) => println!("{}", body),
                    Err(e) => {
                        eprintln!("{RED}Error: {e}{RESET}");
                        std::process::exit(1);
                    }
                }
            }
            "--delete" => {
                if args.len() < 3 {
                    eprintln!("{RED}Error: Delete option requires an email ID.{RESET}");
                    std::process::exit(1);
                }
                let email_id = &args[2];
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
    } else {
        let client = match FakeMailClient::new() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{RED}Error initializing client: {e}{RESET}");
                std::process::exit(1);
            }
        };
        repl::run_interactive(client);
    }
}
