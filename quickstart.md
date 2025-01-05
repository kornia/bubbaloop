---
description: Get started with serving Bubbaloop platform
---

# 🚀 Quickstart

## Setup the project

{% stepper %}
{% step %}
#### Install pre-requisites

{% hint style="info" %}
you need to install `cargo` in order to fetch and build necessary packages. If you don't have `cargo`, you can install it by following the instructions on the [official Rust website](https://www.rust-lang.org/tools/install).
{% endhint %}

Install **justfile**: [https://github.com/casey/just?tab=readme-ov-file#linux](https://github.com/casey/just?tab=readme-ov-file#linux)
{% endstep %}

{% step %}
#### Install Dependencies

To get started, ensure all necessary system dependencies

```
just install_deps
```
{% endstep %}

{% step %}
#### Install the project

```
git clone https://github.com/kornia/bubbaloop.git
```
{% endstep %}
{% endstepper %}

## Serve in local

Launch the server via the terminal; it defaults to listening on `0.0.0.0:3000`

```
just serve
```

You might observe something like this:

```bash
[2025-01-04T23:14:46Z INFO bubbaloop::api] 🚀 Starting the server
[2025-01-04T23:14:46Z INFO bubbaloop::api] 🔥 Listening on: 0.0.0.0:3000
[2025-01-04T23:14:46Z INFO bubbaloop::api] 🔧 Press Ctrl+C to stop the server
```

## Serve remotely

Repeat the process about in a remote machine (e.g. in Nvidia Jetson) and give a `HOST`and an `IP` to serve remotely.

```bash
just serve 192.168.1.154 3000
```

## Use the CLI

```bash
just help
```

```bash
Usage: bubbaloop [-h <host>] [-p <port>] <command> [<args>]

Bubbaloop CLI

Options:
  -h, --host        the host to listen on
  -p, --port        the port to listen on
  --help, help      display usage information

Commands:
  compute           Execute local routines on the server
  pipeline          Pipeline management commands
  stats             Get stats about the server
```
