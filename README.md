# MOULArs - A Myst Online: Uru Live (Again) server in Rust

## Introduction
MOULArs is a [DirtSand](https://github.com/H-uru/dirtsand) compatible Myst
Online: Uru Live (Again) server written in Rust.  Although it is based on
DirtSand, it is differentiated by a few key features:
* Memory and error safety means the server is more resilient to crashes and 
  misbehavior caused by clients sending bad or unexpected data.
* Simpler configuration and setup, including automatic File Server manifest
  generation (see below)
* Better cross-platform compatibility.  MOULArs is known to work on both
  Linux and Windows, but it should also work anywhere else Rust and the
  library dependencies can run (macOS, OpenIndiana, *BSD, etc...)

Like DirtSand, MOULArs is designed to work best with the
[H-uru Plasma](https://github.com/H-uru/Plasma) client, but it may work
with other compatible CWE/Plasma clients as well.

## Building the code
Assuming you have [rust](https://www.rust-lang.org/) with cargo already
installed, building is usually as simple as cloning the repo and running
`cargo build`.

For release builds (recommended for production servers), you should build
instead with `cargo build --release`.

## Setting up a server
*... Database TBD ...*

### File Server
Unlike DirtSand, MOULArs includes a manifest generation tool that only
requires you to provide files in an expected directory structure, and it
will automatically generate manifests and encrypt/compress the files as
appropriate.  To use it, run
`mfs_tool update <path to data root> [--python <path to python executable>]`.
This will update existing manifests with any changes and new files, as well as
producing new manifests (both for the initial server setup and for newly added
client flavors, ages, etc.).  When the `--python` parameter is also used, it
will also compile any .py source files in the `Python` directory and produce
an encrypted .pak file for the Auth server to send to clients.

To ensure all required manifests are properly generated, you should provide
the files in the following structure:

```
<data root> (Configured via moulars.toml)
|- client/
|  |- windows_ia32/
|  |  |- external/
|  |  |  |- UruExplorer.exe  (External build)
|  |  |  |- UruLauncher.exe  (External build)
|  |  |  |- vcredist_x86.exe
|  |  |  `- Other .dll, .pdb, etc files for external build
|  |  `- internal/
|  |     |- plUruExplorer.exe  (Internal build)
|  |     |- plUruLauncher.exe  (Internal build)
|  |     |- vcredist_x86.exe
|  |     `- Other .dll, .pdb, etc files for internal build
|  `- windows_x64/
|     |- external/
|     |  |- UruExplorer.exe  (External x64 build)
|     |  |- UruLauncher.exe  (External x64 build)
|     |  |- vcredist_x64.exe
|     |  `- Other .dll, .pdb, etc files for external build
|     `- internal/
|        |- plUruExplorer.exe  (Internal x64 build)
|        |- plUruLauncher.exe  (Internal x64 build)
|        |- vcredist_x64.exe
|        `- Other .dll, .pdb, etc files for internal build
|- avi/
|  `- video files (.webm, .bik, etc)
|- dat/
|  `- .age, .prp, .fni, .p2f, etc files
|- Python/
|  `- .py source files
|- SDL/
|  `- .sdl files
`- sfx/
   `- .ogg files required by PRPs
```
