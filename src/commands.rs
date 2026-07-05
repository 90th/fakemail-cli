use crate::client::{ClientError, FakeMailClient};
use crate::html::get_remaining_time;

pub const GREEN: &str = "\x1B[32m";
pub const RED: &str = "\x1B[31m";
pub const YELLOW: &str = "\x1B[33m";
pub const RESET: &str = "\x1B[0m";

pub fn print_mailbox(client: &FakeMailClient, email_label: &str, password_label: &str) {
    println!("{email_label:<16}{GREEN}{}{RESET}", client.email());
    if !client.password().is_empty() {
        println!("{password_label:<16}{GREEN}{}{RESET}", client.password());
    }
}

pub fn print_status(client: &mut FakeMailClient, password_label: &str) {
    print_mailbox(client, "Active Email:", password_label);
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

pub fn print_inbox_json(emails: &serde_json::Value) -> Result<(), ClientError> {
    println!("{}", serde_json::to_string_pretty(emails)?);
    Ok(())
}

pub fn print_inbox_table(emails: &serde_json::Value) {
    if let Some(arr) = emails.as_array() {
        if arr.is_empty() {
            println!("Inbox is empty.");
            return;
        }

        println!("\nFound {} email(s):", arr.len());
        println!("{}", "-".repeat(60));
        for mail in arr {
            println!(
                "ID:      {}",
                mail.get("id").and_then(|v| v.as_i64()).unwrap_or(0)
            );
            println!(
                "From:    {}",
                mail.get("od").and_then(|v| v.as_str()).unwrap_or("")
            );
            println!(
                "Date:    {}",
                mail.get("kdy").and_then(|v| v.as_str()).unwrap_or("")
            );
            println!(
                "Subject: {}",
                mail.get("predmet").and_then(|v| v.as_str()).unwrap_or("")
            );
            println!("{}", "-".repeat(60));
        }
    } else {
        println!("Unexpected response structure from server.");
    }
}
