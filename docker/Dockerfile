### ---------------------------------------------------
### How to build the image
## $ docker build -t bubbaloop:local .
### How to run the image
## $ docker run -it \
##              # Define the name of the container
##              --name bubbaloop-server \
##              # Define the port mapping
##              -p 3000:3000 \
##              # Define the environment variables
##              -e RUST_LOG=debug \
##              # Map the local data folder to the container
##              # --volume /tmp/data/:/tmp/data/ \
##              # Define the image and tag
##              bubbaloop:local
### ---------------------------------------------------

FROM rust:1.83.0-bookworm AS build

# create a new empty shell project
RUN USER=root cargo new --bin build-bubbaloop
WORKDIR /build-bubbaloop

# copy the manifests and build files
COPY ./Cargo.toml ./Cross.toml ./build.rs ./bubbaloop.ron ./

RUN \
    # Install system dependencies
    apt-get update \
    && apt-get install -y --no-install-recommends \
    libgstreamer1.0-dev \
    libgstreamer-plugins-base1.0-dev \
    # cache dependencies
    && cargo build --release \
    # Clean up
    && apt-get clean \
    ## Remove apt cache
    && rm -rf /var/lib/apt/lists/* \
    ## Remove the dummy project (used to cache dependencies)
    && rm -rf src/ \
    ## Remove the dummy project's build artifacts
    && rm ./target/release/deps/bubbaloop*

# copy the source of the project
COPY ./src ./src

# compile the server binary
RUN cargo build --release --bin serve

# production image
FROM rust:1.83.0-slim-bookworm AS production

# define the workdir
WORKDIR /app/

RUN \
    # Install system dependencies
    apt-get update \
    && apt-get install -y --no-install-recommends \
    # GStreamer required for video-stream handling
    libgstreamer-plugins-base1.0 \
    # Clean up
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* 

# copy the build artifact from the build stage
COPY --from=build /build-bubbaloop/target/release/serve .

# Set entrypoint
ENTRYPOINT ["./serve"]
# Set the default command to run when starting the container
CMD ["-p", "3000"]
