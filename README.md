# recent-messages2

![Build CI status](https://github.com/robotty/recent-messages2/workflows/Build/badge.svg)

This is a rewrite of the [version 1 recent-messages service](https://github.com/robotty/recent-messages).

See https://recent-messages.robotty.de/ for all kinds of information you might want.

## Build

1. [Install Rust](https://www.rust-lang.org/tools/install)
3. `git clone https://github.com/robotty/recent-messages2.git && cd recent-messages2`
4. `cargo build --release`
5. The binary application will be ready in `./target/release/recent-messages2` (On Windows with the additional `.exe` suffix). The binary is statically linked and so can be moved to other directories or sent to remote machines without need for additional files.

## Install

The `config.toml` is expected to be in the working directory of the process. Edit it to your use case before first startup:

```
editor config.toml
```

The binary can be run with any process manager in the background (systemd etc.), or you can dockerize it. For testing purposes, you can use `cargo run --release`.

A sample file for running it as a systemd unit is provided as `recent-messages2.service`.

```
cp ./recent-messages2.service /etc/systemd/system/recent-messages2.service
```

Now edit the service file to reflect your setup:

```
sudo editor /etc/systemd/system/recent-messages2.service
```

And start the service.

```
sudo systemctl daemon-reload
sudo systemctl enable --now recent-messages2.service
```

View log output/service status:

```
sudo journalctl -efu recent-messages2.service
sudo systemctl status recent-messages2.service
```

Also, wherever you placed the service's working directory, ensure there is a directory called `messages` that is writable for the service. Messages will be persisted there between restarts.

## Web

Instructions for setting up the static website (like the "official" https://recent-messages.robotty.de/) are found in the [README in the `./web` directory of this repo](./web/README.md).
There you can also find an example nginx config.

## Monitoring

A prometheus metrics endpoint is exposed at `/api/v2/metrics`. You can import the `grafana-dashboard.json` in the repository as a dashboard template into a Grafana instance.
