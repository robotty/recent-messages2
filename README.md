# recent-messages2

![Build CI status](https://github.com/robotty/recent-messages2/workflows/Build/badge.svg)

This is a rewrite of the [version 1 recent-messages service](https://github.com/robotty/recent-messages).

See https://recent-messages.robotty.de/ for all kinds of information you might want.

## Build

1. [Install Rust](https://www.rust-lang.org/tools/install)
2. On Debian and Ubuntu: `sudo apt install build-essential libssl-dev pkg-config`, for other operating systems I recommend you just try to proceed with step three and then try to fix the missing compiler programs/system libraries as they pop up
3. `git clone https://github.com/robotty/recent-messages2.git && cd recent-messages2`
4. `cargo build --release`
5. The binary application will be ready in `./target/release/recent-messages2` (On Windows with the additional `.exe` suffix). The binary is statically linked and so can be moved to other directories or sent to remote machines without need for additional files.

## Database setup

This service uses a PostgreSQL database server for persistence.

Your Postgres server requires the TimescaleDB extension, at least version 2.19.3. [Follow the installation instructions
here.](https://www.tigerdata.com/docs/get-started/choose-your-path/install-timescaledb). The TimescaleDB packages from
Debian/Ubuntu cannot be used, since they install the Apache-2.0 license edition of TimescaleDB, while we require the
["community edition"](https://www.tigerdata.com/docs/get-started/choose-your-path/timescaledb-editions).

After you have completed the TimescaleDB setup steps, restart PostgreSQL:

```bash
sudo systemctl restart postgresql
```

I recommend you set up a system user like this:

```bash
sudo adduser --system --home /opt/recent-messages \
  --shell /bin/false --no-create-home --group \
  --disabled-password --disabled-login  \
  recent_messages
```

Then, create the database, and a user under the same name as the system user (the system user will be able to use the
database user automatically without passwords or further configuration when connecting via unix domain sockets):

```bash
sudo -u postgres psql
#> ALTER SYSTEM SET timescaledb.telemetry_level = 'off';
#> SELECT pg_reload_conf();
#> CREATE USER recent_messages;
#> CREATE DATABASE recent_messages OWNER recent_messages;
#> \c recent_messages
#> CREATE EXTENSION IF NOT EXISTS timescaledb;
#> \q
```

## Install

The `config.toml` is expected to be in the working directory of the process. Edit it to your use case before first startup:

```bash
editor config.toml
```

The binary can be run with any process manager in the background (systemd etc.), or you can dockerize it. For testing purposes, you can use `cargo run --release`.

A sample file for running it as a systemd unit is provided as `recent-messages2.service`.

```bash
cp ./recent-messages2.service /etc/systemd/system/recent-messages2.service
```

Now edit the service file to reflect your setup:

```bash
sudo editor /etc/systemd/system/recent-messages2.service
```

And start the service.

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now recent-messages2.service
```

View log output/service status:

```bash
sudo journalctl -efu recent-messages2.service
sudo systemctl status recent-messages2.service
```

## Web

Instructions for setting up the static website (like the "official" https://recent-messages.robotty.de/) are found in the [README in the `./web` directory of this repo](./web/README.md).
There you can also find an example nginx config.

## Monitoring

A prometheus metrics endpoint is exposed at `/api/v2/metrics`. You can import the `grafana-dashboard.json` in the repository as a dashboard template into a Grafana instance.
