LXA TAC System Daemon
=====================

![Web Interface Screenshot](screenshots/web-interface.png?raw=true "Web Interface")

This piece of software provides an interface between the hardware of the
[Linux Automation GmbH Test Automation Controller (LXA TAC)](https://www.linux-automation.com/en/products/lxa-tac.html)
and the user.
It runs the interface on the TACs LCD display, as well as the web server.
The web server provides an interactive web interface as well as an API
for scripting purposes.


Building outside of `meta-lxatac`
---------------------------------

We will first have a look on how to build the `tacd` for your host PC,
as it is easier than building for the LXA TAC itself, then we will go
into how to build for the real hardware.

### General Setup

#### Install `rust` and `cargo`

We do not require a particularly recent version of Rust, so you may have some
luck installing `rust` and `cargo` from your distribution's package
repositories. If not you may have to resort to installing via
[`rustup`](https://www.rust-lang.org/tools/install) (which may also be
available from distribution repositories).

#### Install `npm`

We use [`npm`](https://github.com/npm/cli) to build the LXA TAC web interface.
You may again have some luck installing it from distribution repositories or
have to resort to installing via `curl … | sh` as documented in the README
linked above.

#### Build web interface

The `tacd` serves a React-Based web interface to interactively remote control
the LXA TAC.
If you want to use this web interface with your `tacd` build you should build it
from source using the dark witchcraft that is javascript dependency management:

    $ cd web
    $ npm install .
    $ npm run build

### Building for your PC

The `tacd` contains stubs that make building a stripped-down version for your
host PC possible. These can be useful for quickly checking if a change
compiles, testing changes in the web interface or running unit tests.

#### Run `tacd` on your PC

The `tacd` heavily relies on a lot of hardware and files being present on the
TAC, this means that the full `tacd` can not run on a non-TAC system.

You can however run a stripped-down version by using:

    $ cargo run --features=demo_mode --no-default-features

Note that rust will complain very loudly about a lot of dead code,
which is not used when building for PC but used on the TAC.

#### Unit tests

While the test coverage is not great yet ([PR](https://github.com/linux-automation/tacd/pulls)s
 welcome!) there are some unit tests that can help find regressions.
Run them using:

    $ cargo test --no-default-features

### Build `tacd` for the TAC

To cross-compile for the LXA TAC you will need to build and install a cross
SDK. If you only want to test a little patch it may be easier to use the
yocto `devtool` in [`meta-lxatac`](https://github.com/linux-automation/meta-lxatac),
instead, which does however have the drawback of longer build times.

#### Add rust toolchain

To build outside of `meta-lxatac` you first need to install the respective rust
toolchain:

    $ rustup target add armv7-unknown-linux-gnueabihf

#### Install the SDK

Next you will have to build an SDK using `meta-lxatac`that includes `libiio`:

    $ bitbake -c do_populate_sdk lxatac-core-image-iio

And install it on your host PC.

To build using the SDK you will have to `souce` it according to the yocto SDK
documentation and add the following to the `.cargo/config.toml`:

    [target.armv7-unknown-linux-gnueabihf]
    linker = "[PATH_TO_YOUR_INSTALLED_SDK]/sysroots/x86_64-oesdk-linux/usr/bin/arm-oe-linux-gnueabi/arm-oe-linux-gnueabi-gcc"
    rustflags = [
        "-C", "link-arg=-mthumb",
        "-C", "link-arg=-mfpu=neon-vfpv4",
        "-C", "link-arg=-mfloat-abi=hard",
        "-C", "link-arg=-mcpu=cortex-a7",
        "-C", "link-arg=--sysroot=[PATH_TO_YOUR_INSTALLED_SDK]/sysroots/cortexa7t2hf-neon-vfpv4-oe-linux-gnueabi",
    ]

Remember to update *both* paths so that they point to your installed SDK.
Also remember to always source the SDK activation script before building for
the LXA TAC (and using a shell without a sourced SDK when building for the host
PC).

#### Building

Then, you can use `cargo build --target armv7-unknown-linux-gnueabihf` to
compile the `tacd`.
The resulting binary is placed in `target/armv7-unknown-linux-gnueabihf/release/tacd`
and contains everything required to run the tacd, including the web interface.
It can thus just be copied to your LXA TAC and run instead of the existing
`tacd` (remember to `systemctl stop tacd` the already running instance).

Contributing
------------

We are always open for outside contributions, just make sure to follow these
guidelines:

- Use a somewhat recent stable rust release for testing
- Use `cargo fmt` after every change to the rust codebase
- Use `cargo deny check license` if you have introduced new dependencies to
  check if they (or their dependencies) introduce license issues.
- Use `npx prettier@=2.8.8 --write .` (in the web directory) after every
  change to the web codebase.
