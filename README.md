# Buttplug Rust Intiface CLI Utility

[![Patreon donate button](https://img.shields.io/badge/patreon-donate-yellow.svg)](https://www.patreon.com/qdot)
[![Github donate button](https://img.shields.io/badge/github-donate-ff69b4.svg)](https://www.github.com/sponsors/qdot)
[![Discord](https://img.shields.io/discord/353303527587708932.svg?logo=discord)](https://discord.buttplug.io)
[![Twitter](https://img.shields.io/twitter/follow/buttplugio.svg?style=social&logo=twitter)](https://twitter.com/buttplugio)

![Intiface CLI Build](https://github.com/intiface/intiface-cli-rs/workflows/Intiface%20CLI%20Build/badge.svg)  ![crates.io](https://img.shields.io/crates/v/intiface-cli.svg)


<p align="center">
  <img src="https://raw.githubusercontent.com/buttplugio/buttplug-rs/dev/buttplug/docs/buttplug_rust_docs.png">
</p>

CLI for Intiface/Buttplug

Basically just a front-end for
[buttplug-rs](https://github.com/buttplugio/buttplug-rs), but since we're trying
to not make people install a program named "Buttplug" on their computers, here
we are.

While this program can be used standalone, it will mostly be featured
as a backend/engine for Intiface Desktop.

## Running

Command line options are as follows:

| Option | Description |
| --------- | --------- |
| `version` | Print version and exit |
| `serverversion` | Print version and exit (kept for legacy reasons) |
| `generatecert` | Generate self signed SSL cert (PEM format) and exit |
| `wsallinterfaces` | Websocket servers will listen on all interfaces (versus only on localhost, which is default) |
| `wsinsecureport [port]` | Network port for connecting via non-ssl (ws://) protocols |
| `ipcpipe [name]` | Name for IPC pipe (not yet implemented) |
| `frontendpipe` | Relay output via protobuf to stdout (only used by Intiface Desktop GUI) |
| `servername` | Identifying name server should emit when asked for info |
| `deviceconfig [file]` | Device configuration file to load (if omitted, uses internal) |
| `userdeviceconfig [file]` | User device configuration file to load (if omitted, none used) |
| `pingtime [number]` | Milliseconds for ping time limit of server (if omitted, set to 0) |
| `stayopen` | Stay open between connections (needed for Windows due to device disconnect issues) |
| `log` | Level of logs to output by default (if omitted, set to None) |

For example, to run the server on an insecure websocket at port 12345:

`intiface-cli --wsinsecureport 12345`

## Compiling

For compiling on all platforms the protobuf compiler (protoc) is required. On
Windows and macOS, this can either be retreived via Chocolatey or Homebrew,
respectively. On Debian linux, the `protobuf-compiler` package can be used.
Other linux distros most likely have a similar package.

Linux will have extra compilation dependency requirements via
[buttplug-rs](https://github.com/buttplugio/buttplug-rs). For pacakges required,
please check there.

## Contributing

Right now, we mostly need code/API style reviews and feedback. We
don't really have any good bite-sized chunks to mentor the
implementation yet, but one we do, those will be marked "Help Wanted"
in our [github
issues](https://github.com/buttplugio/buttplug-rs/issues).

As we need money to keep up with supporting the latest and greatest hardware, we
also have multiple ways to donate!

- [Patreon](https://patreon.com/qdot)
- [Github Sponsors](https://github.com/sponsors/qdot)
- [Ko-Fi](https://ko-fi.com/qdot76367)

## License

Buttplug is BSD licensed.

    Copyright (c) 2016-2021, Nonpolynomial Labs, LLC
    All rights reserved.

    Redistribution and use in source and binary forms, with or without
    modification, are permitted provided that the following conditions are met:

    * Redistributions of source code must retain the above copyright notice, this
      list of conditions and the following disclaimer.

    * Redistributions in binary form must reproduce the above copyright notice,
      this list of conditions and the following disclaimer in the documentation
      and/or other materials provided with the distribution.

    * Neither the name of buttplug nor the names of its
      contributors may be used to endorse or promote products derived from
      this software without specific prior written permission.

    THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
    AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
    IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
    DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
    FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
    DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
    SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
    CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
    OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
    OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
