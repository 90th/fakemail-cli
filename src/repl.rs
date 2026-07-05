use crate::client::FakeMailClient;
use crate::html::{get_remaining_time, parse_duration};
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::io::{self, Write};

const GREEN: &str = "\x1B[32m";
const RED: &str = "\x1B[31m";
const YELLOW: &str = "\x1B[33m";
const RESET: &str = "\x1B[0m";

pub fn run_interactive(mut client: FakeMailClient) {
    println!("============================================================");
    println!("  FakeMail CLI Client");
    println!("============================================================");
    println!("Current Inbox: {GREEN}{}{RESET}", client.email);
    println!("Type 'help' or '?' to list commands. (Use Up/Down arrows for command history)\n");

    let mut rl = match DefaultEditor::new() {
        Ok(editor) => editor,
        Err(e) => {
            eprintln!("Failed to initialize line editor: {}", e);
            return;
        }
    };

    loop {
        let prompt = format!("{YELLOW}{}{RESET} > ", client.email);
        let readline = rl.readline(&prompt);
        match readline {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(trimmed);

                let mut parts = trimmed.splitn(2, ' ');
                let cmd = parts.next().unwrap().to_lowercase();
                let arg = parts.next().unwrap_or("").trim();

                match cmd.as_str() {
                    "q" | "quit" | "exit" => {
                        println!("Goodbye!");
                        break;
                    }
                    "h" | "help" | "?" => {
                        println!("Available commands:");
                        println!("  status          Show current active email address & lifetime");
                        println!("  new             Generate a completely new random mailbox");
                        println!("  custom <name>   Change the prefix of your email address");
                        println!(
                            "  extend <time>   Extend mailbox lifetime (e.g. 10m, 1d, 3d, 5d, 1w, 2w)"
                        );
                        println!("  list / ls       Check inbox and list emails");
                        println!("  read <id>       Read the content of a specific email by ID");
                        println!("  delete <id>     Delete a specific email by ID");
                        println!("  clear / cls     Clear the terminal screen");
                        println!("  exit / quit     Exit the CLI");
                    }
                    "status" => {
                        println!("Active Email:   {GREEN}{}{RESET}", client.email);
                        if !client.password.is_empty() {
                            println!("Temp Password:  {GREEN}{}{RESET}", client.password);
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
                    "new" => {
                        println!("Initializing new mailbox...");
                        match client.init_new_session() {
                            Ok(_) => {
                                println!("New Email:      {GREEN}{}{RESET}", client.email);
                                if !client.password.is_empty() {
                                    println!("Password:       {GREEN}{}{RESET}", client.password);
                                }
                            }
                            Err(e) => println!("{RED}Error: {e}{RESET}"),
                        }
                    }
                    "custom" => {
                        if arg.is_empty() {
                            println!("Usage: custom <username>");
                            continue;
                        }
                        println!("Changing username to '{}'...", arg);
                        match client.set_custom_username(arg) {
                            Ok(new_email) => {
                                println!("Active Email changed to: {GREEN}{new_email}{RESET}")
                            }
                            Err(e) => println!("{RED}Error: {e}{RESET}"),
                        }
                    }
                    "extend" => {
                        if arg.is_empty() {
                            println!("Usage: extend <duration>");
                            println!("Supported values: 10m, 1d, 3d, 5d, 1w, 2w, or raw seconds.");
                            continue;
                        }
                        match parse_duration(arg) {
                            Ok(secs) => {
                                println!("Extending mailbox lifetime by {} seconds...", secs);
                                match client.extend_lifetime(secs) {
                                    Ok(_) => {
                                        println!("{GREEN}Lifetime extended successfully.{RESET}")
                                    }
                                    Err(e) => println!("{RED}Error: {e}{RESET}"),
                                }
                            }
                            Err(e) => println!("{RED}Error: {e}{RESET}"),
                        }
                    }
                    "list" | "ls" => {
                        println!("Checking inbox...");
                        match client.get_inbox() {
                            Ok(emails) => {
                                if let Some(arr) = emails.as_array() {
                                    if arr.is_empty() {
                                        println!("Inbox is empty.");
                                    } else {
                                        println!("\nFound {} email(s):", arr.len());
                                        println!("{}", "-".repeat(60));
                                        for mail in arr {
                                            println!(
                                                "ID:      {}",
                                                mail.get("id")
                                                    .and_then(|v: &serde_json::Value| v.as_i64())
                                                    .unwrap_or(0)
                                            );
                                            println!(
                                                "From:    {}",
                                                mail.get("od")
                                                    .and_then(|v: &serde_json::Value| v.as_str())
                                                    .unwrap_or("")
                                            );
                                            println!(
                                                "Date:    {}",
                                                mail.get("kdy")
                                                    .and_then(|v: &serde_json::Value| v.as_str())
                                                    .unwrap_or("")
                                            );
                                            println!(
                                                "Subject: {}",
                                                mail.get("predmet")
                                                    .and_then(|v: &serde_json::Value| v.as_str())
                                                    .unwrap_or("")
                                            );
                                            println!("{}", "-".repeat(60));
                                        }
                                    }
                                } else {
                                    println!("Unexpected response structure from server.");
                                }
                            }
                            Err(e) => println!("{RED}Error: {e}{RESET}"),
                        }
                    }
                    "read" => {
                        if arg.is_empty() {
                            println!("Usage: read <email_id>");
                            continue;
                        }
                        println!("Fetching email {}...", arg);
                        match client.read_email(arg) {
                            Ok(body) => {
                                println!("\n{}", "=".repeat(60));
                                println!("{}", body);
                                println!("{}\n", "=".repeat(60));
                            }
                            Err(e) => println!("{RED}Error: {e}{RESET}"),
                        }
                    }
                    "delete" => {
                        if arg.is_empty() {
                            println!("Usage: delete <email_id>");
                            continue;
                        }
                        println!("Deleting email {}...", arg);
                        match client.delete_email(arg) {
                            Ok(_) => println!("{GREEN}Email deleted successfully.{RESET}"),
                            Err(e) => println!("{RED}Error: {e}{RESET}"),
                        }
                    }
                    "clear" | "cls" => {
                        print!("\x1B[2J\x1B[1;1H");
                        let _ = io::stdout().flush();
                    }
                    _ => {
                        println!(
                            "{RED}Unknown command: '{cmd}'. Type 'help' to see list of commands.{RESET}"
                        );
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("{YELLOW}Ctrl+C received. Exiting...{RESET}");
                break;
            }
            Err(ReadlineError::Eof) => {
                println!("{YELLOW}Ctrl+D received. Exiting...{RESET}");
                break;
            }
            Err(err) => {
                println!("{RED}Error reading line: {:?}{RESET}", err);
                break;
            }
        }
    }
}
