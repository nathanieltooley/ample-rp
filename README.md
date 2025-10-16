# Ample
A simple Discord Rich Presence application and LastFM scrobbler for Apple Music. Currently only supporting Windows with plans to support Linux.

<img width="296" height="642" alt="rich presence example" src="https://github.com/user-attachments/assets/068ac9a8-65fc-4a1c-8699-81316de2303a" />

## Installation
Simply download and extract the latest [release](https://github.com/nathanieltooley/amp-rp/releases) to a folder of your choosing.
Ample runs in the background so no windows will appear when executed. It's highly recommended to set up some way to autostart the
application when the computer boots. For Windows, look into "Startup Apps" in the settings, the autostart folder, or the Windows "Task Scheduler."

## LastFM Integration
Ample supports scrobbling of songs played on Apple Music. It also currently (may change) way of getting cover art.
Thus without LastFM support enabled, your discord status will only show the song, artist, and album names.

In order for LastFM support to be enabled, you need to provide your LastFM username, password, API Key, and API secret.
[Click here](https://www.last.fm/api/authentication) for more info about registering an API key and secret and [here](https://www.last.fm/api/accounts)
to access already created API keys and secrets.

There are two ways of providing this info to Ample. The first is by setting the following environment variables:
- AMPLE_FM_USERNAME
- AMPLE_FM_PASSWORD
- AMPLE_FM_API_KEY
- AMPLE_FM_SECRET

These can also be provided in an .env file, however this file needs to be in the same folder as the executable.

If you feel uncomfortable keeping this info in a plain text file, the second way uses your platforms credential / secret manager.
For Windows, this is the [Credential Manager](https://support.microsoft.com/en-us/windows/credential-manager-in-windows-1b5c916a-6a16-889f-8581-fc16e8165ac0).
For other platforms, refer to [keyring's supported options](https://crates.io/crates/keyring) under the header **Platforms**.

Ample will look for the secret in an entry called **ampleSecret** and the password in an entry called **amplePassword**

## Troubleshooting
Logs will be stored on Windows in "AppData\Roaming\ample\config\logs", and on Linux at "~/.config/ample/config/logs."
Setting the environment variable "AMPLE_DEBUG" will print debug logging info.

## Building from source
You will need [Rust](https://rustup.rs/) installed. After that, clone the repo onto your computer.

To build the release version of the project, run:
```
cargo build --package ample --release --features headless
```
If you want a console window to appear, you can omit the "headless" feature and run
```
cargo build --package ample --release
```
