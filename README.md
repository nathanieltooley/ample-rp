# Ample
A simple Discord Rich Presence application and LastFM scrobbler for Apple Music on Windows.

<img width="280" height="112" alt="Discord Rich Presence example. It displays the Discord status of an individual, showing that they are currently listening to Twilight Funzone, by the band Macseal, on Apple Music" src="https://github.com/user-attachments/assets/aed3e3c8-028d-41ba-867e-d5b4909fa039" />

## Installation
Simply download and extract the latest [release](https://github.com/nathanieltooley/amp-rp/releases) to a folder of your choosing.
Ample runs in the background so no windows will appear when executed. It's highly recommended to set up some way to autostart the
application when the computer boots. For Windows, look into "Startup Apps" in the settings, the autostart folder, or the Windows "Task Scheduler."

## LastFM Integration
Ample supports scrobbling of songs played on Apple Music. While Ample can provide most features without LastFM support, the public API used for album cover images
([Cover Art Archive](https://coverartarchive.org/)) is subject to more rate limiting while LastFM is more lenient. It may be the case using LastFM's api results
in better performance though it should usually be negligible. 

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

Ample will look for the secret in an entry called **ampleSecret** and the password in an entry called **amplePassword**. However, the *username* and *api_key* still need to be
given using environment variables.

## Troubleshooting
Logs will be stored on Windows in `AppData\Roaming\ample\config\logs`, and on Linux at `~/.config/ample/config/logs.`
Setting the environment variable "AMPLE_DEBUG" will print debug logging info but will require resetting the program.

## Configuration
The config file is located at `AppData\Roaming\ample\config\ample_config.json` on Windows, and `~/.config/ample/config/ample_config.json` on Linux.

- `wait_for_discord` (bool): If true, Ample will pause execution until Discord can be connected to. If false, Ample will attempt to scrobble without Discord. If that fails, then the program will idle. Default value: `false`
- `valid_media_sources` (list): A list of executable names that Ample will listen to and send to Discord and LastFM. Default value: `["AppleMusic.exe"]`

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
