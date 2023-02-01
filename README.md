# DHT22 to Grafana (Graphite)

A small Rust service for Raspberry Pi that reads data from a connected DHT22 sensor and posts it to a Graphite instance (on Grafana Cloud for example).

## Pre-requisites

- Raspberry Pi
- DHT22
- Grafana Cloud or a self-hosted Graphite instance

## Installation - compiling from source

On your Raspberry Pi:

1. Clone the repo `gh repo clone keturiosakys/raspberry-temperature-monitoring`
2. `cd` into it
3. Run `cargo build --release`
4. Grab the compiled binary from the `target/` directory

### Cross-compiling

Due to long Rust compilation times I would recommend cross-compiling the code on your main machine and porting over the compiled binary to the Raspberry Pi.

Use [`cargo-zigbuild`](https://crates.io/crates/cargo-zigbuild) (which uses the `zig` linker) or [`cross`](https://github.com/cross-rs/cross) (which uses Docker to provide the toolchain) to compile for your Raspberry Pi CPU architecture with minimal setup.

## How it works

`rpi-monitoring` compiles to a `monitoring` binary that runs as any CLI application. Under the hood it uses the simple but reliable [dht22_pi](https://github.com/michaelfletchercgy/dht22_pi/) crate to read the actual sensor.

You can run `monitoring check --pin <GPIO_PIN>` to sample data from your connected DHT22 sensor and verify that it's working.

The `monitoring serve` is the command that can run in the background sampling and posting the temperature data to your Graphite instance.

It requires a Graphite endpoint and a Grafana API key passed in as flags as well as a `sensors.yaml` file to be available in the same directory.

`sensors.yaml` file lists and labels all the connected DHT22 sensors.

```yaml
- name: kitchen # label, must be all lowercase, no spaces
  pin: 4 # GPIO pin it's connected to
```

You can set up the executable as a systemd service - there's an example `monitoring.service` in the repository!

Please post any questions or report any issues in the Github Issues of this repo.
