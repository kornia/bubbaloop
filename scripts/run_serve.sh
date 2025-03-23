#! /bin/bash

export RUST_LOG=${RUST_LOG:-debug}

# get the local ip address
HOST=$(hostname -I | awk '{print $1}')
PORT=3000

# script to run the serve binary on the target machine
./serve --host $HOST --port $PORT