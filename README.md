# fakemail-cli <img src="https://fakemail.b-cdn.net/img/fake-mail.png" height="38" align="right" />

lightweight, zero-async rust cli for fakemail.net. stateful, clean, fast.

## why

sometimes you just need a temporary inbox in the terminal to verify a signup.

## install

requires cargo.

```sh
cargo build --release
```

executable compiles to `target/release/fakemail-cli`.

## usage

run it without arguments to open the interactive shell:
```sh
./fakemail-cli
```

or use the commands directly:

```sh
./fakemail-cli --status          # show active mail, password, and remaining time
./fakemail-cli --new             # generate a brand new mailbox
./fakemail-cli --custom <name>   # change prefix (e.g. user123@domain)
./fakemail-cli --extend <time>   # extend lifetime (e.g. 10m, 1d, 3d, 5d, 1w, 2w)
./fakemail-cli --list            # list received emails in json
./fakemail-cli --read <id>       # view raw parsed email body
./fakemail-cli --delete <id>     # delete an email
```

## design

- **session state**: saved in `~/.fakemail_cli_session.json` so your active mailbox persists across commands.
- **stealth**: uses real browser user-agent headers to avoid rate-limits and bot triggers.
- **zero-dependency parsing**: does not use a heavy regex crate or html parser. uses simple standard library state machines for fast compile times.
